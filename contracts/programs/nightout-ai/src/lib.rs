use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use std::collections::HashMap;

declare_id!("NightOutAi7xZ9KqF5YvB3tH8mP2WjC4sR1nD6eL9oA8bX3Y");

#[program]
pub mod nightout_ai {
    use super::*;

    pub fn initialize_platform(
        ctx: Context<InitializePlatform>,
        platform_fee: u64,
        reputation_threshold: u64,
        stake_amount: u64,
    ) -> Result<()> {
        let platform = &mut ctx.accounts.platform;
        platform.authority = ctx.accounts.authority.key();
        platform.platform_fee = platform_fee;
        platform.reputation_threshold = reputation_threshold;
        platform.stake_amount = stake_amount;
        platform.total_events = 0;
        platform.total_recommendations = 0;
        platform.total_venue_partners = 0;
        platform.treasury_balance = 0;
        platform.ai_oracle = ctx.accounts.ai_oracle.key();
        platform.bump = *ctx.bumps.get("platform").unwrap();

        emit!(PlatformInitialized {
            authority: platform.authority,
            platform_fee: platform.platform_fee,
            stake_amount: platform.stake_amount,
        });

        Ok(())
    }

    pub fn register_user(
        ctx: Context<RegisterUser>,
        location: String,
        preferences: Vec<String>,
        spending_range: [u64; 2],
    ) -> Result<()> {
        require!(location.len() <= 100, NightOutError::LocationTooLong);
        require!(preferences.len() <= 10, NightOutError::TooManyPreferences);
        require!(spending_range[0] <= spending_range[1], NightOutError::InvalidSpendingRange);

        let user_account = &mut ctx.accounts.user_account;
        user_account.authority = ctx.accounts.authority.key();
        user_account.location = location;
        user_account.preferences = preferences;
        user_account.spending_range = spending_range;
        user_account.reputation_score = 100; // Starting reputation
        user_account.total_recommendations = 0;
        user_account.successful_recommendations = 0;
        user_account.tokens_earned = 0;
        user_account.tokens_spent = 0;
        user_account.favorite_venues = Vec::new();
        user_account.attended_events = Vec::new();
        user_account.last_activity = Clock::get()?.unix_timestamp;
        user_account.subscription_tier = SubscriptionTier::Free;
        user_account.ai_interaction_count = 0;
        user_account.bump = *ctx.bumps.get("user_account").unwrap();

        let platform = &mut ctx.accounts.platform;
        platform.total_users += 1;

        emit!(UserRegistered {
            user: user_account.authority,
            location: user_account.location.clone(),
            preferences: user_account.preferences.clone(),
        });

        Ok(())
    }

    pub fn register_venue_partner(
        ctx: Context<RegisterVenuePartner>,
        venue_name: String,
        venue_type: VenueType,
        location: String,
        capacity: u32,
        commission_rate: u64,
    ) -> Result<()> {
        require!(venue_name.len() <= 100, NightOutError::VenueNameTooLong);
        require!(location.len() <= 200, NightOutError::LocationTooLong);
        require!(commission_rate <= 5000, NightOutError::CommissionTooHigh); // Max 50%
        require!(capacity > 0, NightOutError::InvalidCapacity);

        let venue = &mut ctx.accounts.venue_account;
        venue.authority = ctx.accounts.authority.key();
        venue.venue_name = venue_name;
        venue.venue_type = venue_type;
        venue.location = location;
        venue.capacity = capacity;
        venue.commission_rate = commission_rate;
        venue.reputation_score = 100;
        venue.total_events = 0;
        venue.total_bookings = 0;
        venue.revenue_generated = 0;
        venue.is_verified = false;
        venue.stake_deposited = 0;
        venue.events = Vec::new();
        venue.ratings = Vec::new();
        venue.features = Vec::new();
        venue.operating_hours = [[0u8; 2]; 7]; // 7 days, [open, close] hours
        venue.bump = *ctx.bumps.get("venue_account").unwrap();

        // Require stake deposit for venue partners
        let stake_transfer = Transfer {
            from: ctx.accounts.venue_token_account.to_account_info(),
            to: ctx.accounts.platform_treasury.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            stake_transfer,
        );

        token::transfer(cpi_ctx, ctx.accounts.platform.stake_amount)?;
        venue.stake_deposited = ctx.accounts.platform.stake_amount;

        let platform = &mut ctx.accounts.platform;
        platform.total_venue_partners += 1;
        platform.treasury_balance += venue.stake_deposited;

        emit!(VenuePartnerRegistered {
            venue: venue.authority,
            venue_name: venue.venue_name.clone(),
            venue_type: venue.venue_type,
            stake_amount: venue.stake_deposited,
        });

        Ok(())
    }

    pub fn create_event(
        ctx: Context<CreateEvent>,
        event_name: String,
        event_type: EventType,
        description: String,
        start_time: i64,
        end_time: i64,
        ticket_price: u64,
        max_capacity: u32,
        tags: Vec<String>,
    ) -> Result<()> {
        require!(event_name.len() <= 100, NightOutError::EventNameTooLong);
        require!(description.len() <= 500, NightOutError::DescriptionTooLong);
        require!(start_time > Clock::get()?.unix_timestamp, NightOutError::EventInPast);
        require!(end_time > start_time, NightOutError::InvalidEventTimes);
        require!(max_capacity > 0, NightOutError::InvalidCapacity);
        require!(tags.len() <= 10, NightOutError::TooManyTags);

        let event = &mut ctx.accounts.event_account;
        event.authority = ctx.accounts.venue.authority;
        event.venue = ctx.accounts.venue.key();
        event.event_name = event_name;
        event.event_type = event_type;
        event.description = description;
        event.start_time = start_time;
        event.end_time = end_time;
        event.ticket_price = ticket_price;
        event.max_capacity = max_capacity;
        event.current_bookings = 0;
        event.tags = tags;
        event.is_active = true;
        event.featured_score = 0;
        event.total_revenue = 0;
        event.attendees = Vec::new();
        event.reviews = Vec::new();
        event.created_at = Clock::get()?.unix_timestamp;
        event.bump = *ctx.bumps.get("event_account").unwrap();

        // Update venue stats
        let venue = &mut ctx.accounts.venue;
        venue.total_events += 1;
        venue.events.push(ctx.accounts.event_account.key());

        // Update platform stats
        let platform = &mut ctx.accounts.platform;
        platform.total_events += 1;

        emit!(EventCreated {
            event: ctx.accounts.event_account.key(),
            venue: venue.key(),
            event_name: event.event_name.clone(),
            start_time: event.start_time,
            ticket_price: event.ticket_price,
        });

        Ok(())
    }

    pub fn request_recommendation(
        ctx: Context<RequestRecommendation>,
        query: String,
        location: String,
        budget_min: u64,
        budget_max: u64,
        event_types: Vec<EventType>,
        time_preference: TimePreference,
    ) -> Result<()> {
        require!(query.len() <= 500, NightOutError::QueryTooLong);
        require!(location.len() <= 100, NightOutError::LocationTooLong);
        require!(budget_min <= budget_max, NightOutError::InvalidBudgetRange);
        require!(event_types.len() <= 5, NightOutError::TooManyEventTypes);

        let recommendation = &mut ctx.accounts.recommendation;
        recommendation.user = ctx.accounts.user.key();
        recommendation.query = query;
        recommendation.location = location;
        recommendation.budget_range = [budget_min, budget_max];
        recommendation.event_types = event_types;
        recommendation.time_preference = time_preference;
        recommendation.status = RecommendationStatus::Pending;
        recommendation.ai_confidence = 0;
        recommendation.recommended_events = Vec::new();
        recommendation.user_feedback = None;
        recommendation.created_at = Clock::get()?.unix_timestamp;
        recommendation.expires_at = Clock::get()?.unix_timestamp + 86400; // 24 hours
        recommendation.bump = *ctx.bumps.get("recommendation").unwrap();

        // Update user stats
        let user = &mut ctx.accounts.user;
        user.total_recommendations += 1;
        user.last_activity = Clock::get()?.unix_timestamp;
        user.ai_interaction_count += 1;

        // Update platform stats
        let platform = &mut ctx.accounts.platform;
        platform.total_recommendations += 1;

        emit!(RecommendationRequested {
            user: user.key(),
            recommendation_id: ctx.accounts.recommendation.key(),
            query: recommendation.query.clone(),
            budget_range: recommendation.budget_range,
        });

        Ok(())
    }

    pub fn process_ai_recommendation(
        ctx: Context<ProcessAIRecommendation>,
        recommended_events: Vec<Pubkey>,
        confidence_scores: Vec<u8>,
        reasoning: String,
    ) -> Result<()> {
        require!(ctx.accounts.ai_oracle.key() == ctx.accounts.platform.ai_oracle, NightOutError::UnauthorizedAI);
        require!(recommended_events.len() == confidence_scores.len(), NightOutError::MismatchedArrays);
        require!(recommended_events.len() <= 10, NightOutError::TooManyRecommendations);
        require!(reasoning.len() <= 1000, NightOutError::ReasoningTooLong);

        let recommendation = &mut ctx.accounts.recommendation;
        require!(recommendation.status == RecommendationStatus::Pending, NightOutError::RecommendationAlreadyProcessed);
        require!(Clock::get()?.unix_timestamp < recommendation.expires_at, NightOutError::RecommendationExpired);

        recommendation.recommended_events = recommended_events.clone();
        recommendation.confidence_scores = confidence_scores.clone();
        recommendation.ai_reasoning = reasoning;
        recommendation.ai_confidence = confidence_scores.iter().sum::<u8>() / confidence_scores.len() as u8;
        recommendation.status = RecommendationStatus::Completed;
        recommendation.processed_at = Some(Clock::get()?.unix_timestamp);

        // Award tokens to user for receiving recommendation
        let user = &mut ctx.accounts.user;
        let base_reward = 10; // Base tokens per recommendation
        let confidence_bonus = (recommendation.ai_confidence as u64 / 10).min(10);
        let total_reward = base_reward + confidence_bonus;

        user.tokens_earned += total_reward;

        emit!(AIRecommendationProcessed {
            user: user.key(),
            recommendation_id: recommendation.key(),
            events_recommended: recommended_events.len() as u32,
            ai_confidence: recommendation.ai_confidence,
            tokens_awarded: total_reward,
        });

        Ok(())
    }

    pub fn book_event(
        ctx: Context<BookEvent>,
        quantity: u32,
        payment_method: PaymentMethod,
    ) -> Result<()> {
        require!(quantity > 0, NightOutError::InvalidQuantity);
        
        let event = &mut ctx.accounts.event;
        require!(event.is_active, NightOutError::EventInactive);
        require!(event.current_bookings + quantity <= event.max_capacity, NightOutError::EventSoldOut);
        require!(Clock::get()?.unix_timestamp < event.start_time, NightOutError::EventAlreadyStarted);

        let booking = &mut ctx.accounts.booking;
        booking.user = ctx.accounts.user.key();
        booking.event = ctx.accounts.event.key();
        booking.venue = event.venue;
        booking.quantity = quantity;
        booking.total_amount = event.ticket_price * quantity as u64;
        booking.payment_method = payment_method;
        booking.status = BookingStatus::Confirmed;
        booking.booked_at = Clock::get()?.unix_timestamp;
        booking.attendance_confirmed = false;
        booking.refund_requested = false;
        booking.bump = *ctx.bumps.get("booking").unwrap();

        // Process payment based on method
        match payment_method {
            PaymentMethod::Token => {
                let payment_transfer = Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.venue_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                };

                let cpi_ctx = CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    payment_transfer,
                );

                // Calculate platform fee
                let platform_fee = (booking.total_amount * ctx.accounts.platform.platform_fee) / 10000;
                let venue_amount = booking.total_amount - platform_fee;

                // Transfer to venue
                token::transfer(cpi_ctx, venue_amount)?;

                // Transfer platform fee to treasury
                let fee_transfer = Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.platform_treasury.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                };

                let fee_cpi_ctx = CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    fee_transfer,
                );

                token::transfer(fee_cpi_ctx, platform_fee)?;

                // Update platform treasury balance
                let platform = &mut ctx.accounts.platform;
                platform.treasury_balance += platform_fee;
            }
            PaymentMethod::Sol => {
                // Handle SOL payments via native transfers
                let platform_fee = (booking.total_amount * ctx.accounts.platform.platform_fee) / 10000;
                let venue_amount = booking.total_amount - platform_fee;

                // Transfer SOL to venue
                **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? -= venue_amount;
                **ctx.accounts.venue.to_account_info().try_borrow_mut_lamports()? += venue_amount;

                // Transfer platform fee to treasury
                **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? -= platform_fee;
                **ctx.accounts.platform_treasury.to_account_info().try_borrow_mut_lamports()? += platform_fee;
            }
        }

        //