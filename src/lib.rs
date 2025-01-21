#![cfg_attr(not(feature = "std"), no_std)]

///! # Archway-Like Reward Pallet
///!
///! This pallet demonstrates a simple reward distribution mechanism. It includes:
///! - A reward pool, which can be topped up by a privileged origin.
///! - A per-block reward for block authors.
///! - A manual claim extrinsic for developers/users (e.g., for contract rewards).
///!
///!

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        dispatch::{DispatchError, DispatchResult},
        pallet_prelude::*,
        traits::{Currency, Get, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Zero;
    use sp_std::marker::PhantomData;

    // ---------------------------------------------
    //  Type aliases & helper definitions
    // ---------------------------------------------

    /// Convenience type alias for the balance of this pallet's currency.
    pub type BalanceOf<T> = <<T as Config>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    // ---------------------------------------------
    //  Pallet Configuration
    // ---------------------------------------------

    /// The pallet's configuration trait. Substrate uses this trait to inject
    /// dependencies (e.g. types, constants, origins) from the runtime.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency mechanism (e.g., Balances) used for rewards.
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// The reward amount automatically distributed per block to the block author.
        /// (Set to `0` if you don't want to use block-based emission.)
        #[pallet::constant]
        type RewardPerBlock: Get<BalanceOf<Self>>;

        /// The origin that is allowed to top-up the reward pool (e.g., governance, root, etc.).
        type RewardManagerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// This is typically your `Balance` type from the runtime (e.g., `u128`).
        type Balance: Parameter + From<u64> + Into<u128> + MaxEncodedLen + Default + Copy;
    }

    // ---------------------------------------------
    //  Genesis Configuration
    // ---------------------------------------------

    /// Pallet genesis configuration. Allows specifying an initial reward pool at chain genesis.
    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        /// Amount of tokens to initialize in the reward pool.
        pub initial_reward_pool: BalanceOf<T>,
        /// Phantom data to ensure type correctness.
        pub _phantom: PhantomData<T>,
    }

    /// Default implementation for GenesisConfig. Sets initial reward pool to zero.
    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                initial_reward_pool: Zero::zero(),
                _phantom: Default::default(),
            }
        }
    }

    /// This block builds the genesis storage using the configuration values
    /// provided in `GenesisConfig`.
    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            RewardPool::<T>::put(self.initial_reward_pool);
            TotalDistributed::<T>::put(Zero::zero());
        }
    }

    // ---------------------------------------------
    //  Pallet Declaration
    // ---------------------------------------------

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    // ---------------------------------------------
    //  Storage Items
    // ---------------------------------------------

    /// The current size of the reward pool. This pool is the source of all
    /// rewards in this pallet (per-block or manual claim).
    #[pallet::storage]
    #[pallet::getter(fn reward_pool)]
    pub type RewardPool<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Tracks the total amount of rewards that have ever been distributed
    /// through this pallet (both block rewards and manual claims).
    #[pallet::storage]
    #[pallet::getter(fn total_distributed)]
    pub type TotalDistributed<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    // ---------------------------------------------
    //  Events
    // ---------------------------------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Reward pool was increased. (amount_added, new_pool_total)
        RewardPoolIncreased(BalanceOf<T>, BalanceOf<T>),
        /// A reward was claimed by an account. (who, amount)
        RewardClaimed(T::AccountId, BalanceOf<T>),
        /// A block reward was distributed. (block_author, amount)
        BlockRewardDistributed(T::AccountId, BalanceOf<T>),
    }

    // ---------------------------------------------
    //  Errors
    // ---------------------------------------------

    #[pallet::error]
    pub enum Error<T> {
        /// Attempting to distribute or claim more than is available in the pool.
        InsufficientRewardPool,
        /// Attempting to claim zero (invalid) or negative (impossible) amount.
        InvalidClaimAmount,
        /// The origin did not match the required origin for this call.
        BadOriginForTopUp,
    }

    // ---------------------------------------------
    //  Hooks: Automatic Block Reward Logic
    // ---------------------------------------------

    /// We use the `on_initialize` hook to distribute a per-block reward
    /// to the block author, if configured (RewardPerBlock > 0).
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_n: T::BlockNumber) -> Weight {
            let reward_per_block = T::RewardPerBlock::get();
            // If the reward is set to zero, do nothing.
            if reward_per_block.is_zero() {
                return 0;
            }

            let pool = Self::reward_pool();

            // If there's not enough in the pool, we skip distributing a block reward.
            // (Alternatively, you could distribute what's left or handle in other ways.)
            if pool < reward_per_block {
                return 0;
            }

            // Get the block author. This depends on your consensus mechanism.
            // In many Substrate setups (e.g., AURA/BABE), `pallet_authorship`
            // or `frame_system` can store the block author.
            //
            // We demonstrate a simplified approach: see if the system pallet
            // provides a block_author function. If it's Some(author), we proceed.
            if let Some(block_author) = frame_system::Pallet::<T>::block_author() {
                // Deduct from the reward pool
                let new_pool = pool - reward_per_block;
                RewardPool::<T>::put(new_pool);

                // Update total distributed
                let total_dist = Self::total_distributed();
                let updated_dist = total_dist + reward_per_block;
                TotalDistributed::<T>::put(updated_dist);

                // Transfer reward to block author
                T::Currency::deposit_creating(&block_author, reward_per_block);

                // Emit event
                Self::deposit_event(Event::BlockRewardDistributed(block_author, reward_per_block));
            }

            // Return some weight cost estimate. The actual weight formula should
            // account for read/write operations. This is a simplified example.
            10_000
        }
    }

    // ---------------------------------------------
    //  Extrinsics
    // ---------------------------------------------

    /// The callable functions (extrinsics) of this pallet.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Top up the reward pool by `amount`. Must come from `RewardManagerOrigin`.
        ///
        /// # Arguments
        /// * `origin` - Must satisfy the `RewardManagerOrigin` (e.g., Root, Council, etc.).
        /// * `amount` - The amount to add to the reward pool.
        #[pallet::weight(10_000)]
        pub fn top_up_pool(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            // Check that the origin is authorized
            T::RewardManagerOrigin::try_origin(origin)
                .map_err(|_| Error::<T>::BadOriginForTopUp)?;

            let pool_before = Self::reward_pool();
            let new_pool = pool_before
                .checked_add(&amount)
                .ok_or(ArithmeticError::Overflow)?;

            // Update the storage
            RewardPool::<T>::put(new_pool);

            // Emit event
            Self::deposit_event(Event::RewardPoolIncreased(amount, new_pool));

            Ok(())
        }

        /// Claim `amount` of tokens from the reward pool (e.g., for developer rewards).
        ///
        /// # Arguments
        /// * `origin` - Any signed account that is eligible to claim (in real systems,
        ///   you'd verify eligibility and usage metrics).
        /// * `amount` - The amount to claim.
        #[pallet::weight(10_000)]
        pub fn claim_reward(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let claimant = ensure_signed(origin)?;

            // Validate the requested amount
            ensure!(!amount.is_zero(), Error::<T>::InvalidClaimAmount);

            // Check if the pool has enough funds
            let pool_before = Self::reward_pool();
            ensure!(pool_before >= amount, Error::<T>::InsufficientRewardPool);

            // Update the pool
            let new_pool = pool_before - amount;
            RewardPool::<T>::put(new_pool);

            // Update the total distributed
            let total_dist_before = Self::total_distributed();
            let new_total_dist = total_dist_before + amount;
            TotalDistributed::<T>::put(new_total_dist);

            // Transfer to the claimant
            T::Currency::deposit_creating(&claimant, amount);

            // Emit event
            Self::deposit_event(Event::RewardClaimed(claimant, amount));
            Ok(())
        }
    }
}

