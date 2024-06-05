#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency, Get},
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{CheckedAdd, CheckedSub};
use codec::{Encode, Decode, MaxEncodedLen};
use scale_info::TypeInfo;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_support::BoundedVec;
use frame_support::traits::ExistenceRequirement;
use sp_runtime::Saturating;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + scale_info::TypeInfo {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
        type StorageRewardPeriod: Get<BlockNumberFor<Self>>;
        type MaxCIDLength: Get<u32>;
        type MaxPeerIDLength: Get<u32>;
        type MonthlyStorageCost: Get<BalanceOf<Self>>;
    }

    type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct StorageRequest<T: Config> {
        cid: BoundedVec<u8, T::MaxCIDLength>,
        uploader: T::AccountId,
        size: u64,
        locked_tokens: BalanceOf<T>,
        start_block: BlockNumberFor<T>,
        peer_id: BoundedVec<u8, T::MaxPeerIDLength>,
    }

	#[pallet::storage]
	#[pallet::getter(fn monthly_storage_cost)]
	pub type MonthlyStorageCost<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn storage_requests)]
    pub type StorageRequests<T: Config> = StorageMap<_, Blake2_128Concat, BoundedVec<u8, T::MaxCIDLength>, StorageRequest<T>>;

    #[pallet::storage]
    #[pallet::getter(fn total_locked_tokens)]
    pub type TotalLockedTokens<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TokensLocked(T::AccountId, BalanceOf<T>),
        TokensUnlocked(T::AccountId, BalanceOf<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        InsufficientBalance,
        StorageRequestNotFound,
        MaxEncodedLenExceeded,
        ExceededStorageLimit,
    }

    #[pallet::call]
	impl<T: Config> Pallet<T>
	where
		BalanceOf<T>: From<u64>,
	{
        #[pallet::weight(T::WeightInfo::lock_tokens())]
		#[pallet::call_index(0)]
		pub fn lock_tokens(
			origin: OriginFor<T>,
			cid: BoundedVec<u8, T::MaxCIDLength>,
			size: u64, // size in GB
			peer_id: BoundedVec<u8, T::MaxPeerIDLength>,
		) -> DispatchResult {
			let uploader = ensure_signed(origin)?;
	
			let monthly_cost = MonthlyStorageCost::<T>::get();
			let required_tokens = monthly_cost.saturating_mul(size.into());
			ensure!(T::Currency::free_balance(&uploader) >= required_tokens, Error::<T>::InsufficientBalance);
	
			T::Currency::reserve(&uploader, required_tokens)?;
			let start_block = <frame_system::Pallet<T>>::block_number();
	
			let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone();
			let bounded_peer_id: BoundedVec<u8, T::MaxPeerIDLength> = peer_id.clone();
	
			let new_request = StorageRequest {
				cid: bounded_cid.clone(),
				uploader: uploader.clone(),
				size,
				locked_tokens: required_tokens,
				start_block,
				peer_id: bounded_peer_id.clone(),
			};
	
			StorageRequests::<T>::insert(&bounded_cid, new_request);
			TotalLockedTokens::<T>::try_mutate(|total| -> Result<(), Error<T>> {
				*total = total.checked_add(&required_tokens).ok_or(Error::<T>::InsufficientBalance)?;
				Ok(())
			})?;
	
			Self::deposit_event(Event::TokensLocked(uploader, required_tokens));
	
			Ok(())
		}

        #[pallet::weight(T::WeightInfo::unlock_tokens())]
        #[pallet::call_index(1)]
        pub fn unlock_tokens(origin: OriginFor<T>, cid: BoundedVec<u8, T::MaxCIDLength>) -> DispatchResult {
            let uploader = ensure_signed(origin)?;

            let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone();
            let request = StorageRequests::<T>::get(&bounded_cid).ok_or(Error::<T>::StorageRequestNotFound)?;
            ensure!(request.uploader == uploader, Error::<T>::InsufficientBalance);

            T::Currency::unreserve(&uploader, request.locked_tokens);
            StorageRequests::<T>::remove(&bounded_cid);
            TotalLockedTokens::<T>::try_mutate(|total| -> Result<(), Error<T>> {
                *total = total.checked_sub(&request.locked_tokens).ok_or(Error::<T>::InsufficientBalance)?;
                Ok(())
            })?;

            Self::deposit_event(Event::TokensUnlocked(uploader, request.locked_tokens));

            Ok(())
        }

        // Additional function to release tokens to storers upon proof submission
        #[pallet::weight(T::WeightInfo::unlock_tokens())]
        #[pallet::call_index(2)]
        pub fn release_tokens(
            origin: OriginFor<T>,
            cid: BoundedVec<u8, T::MaxCIDLength>,
            storer: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone();
            let request = StorageRequests::<T>::get(&bounded_cid).ok_or(Error::<T>::StorageRequestNotFound)?;

            ensure!(request.locked_tokens >= amount, Error::<T>::InsufficientBalance);

            T::Currency::unreserve(&request.uploader, amount);
            T::Currency::transfer(&request.uploader, &storer, amount, ExistenceRequirement::AllowDeath)?;
            
            StorageRequests::<T>::mutate(&bounded_cid, |req| {
                if let Some(r) = req {
                    r.locked_tokens = r.locked_tokens.saturating_sub(amount);
                }
            });
            TotalLockedTokens::<T>::try_mutate(|total| -> Result<(), Error<T>> {
                *total = total.checked_sub(&amount).ok_or(Error::<T>::InsufficientBalance)?;
                Ok(())
            })?;

            Self::deposit_event(Event::TokensUnlocked(request.uploader, amount));

            Ok(())
        }

		#[pallet::weight(T::WeightInfo::update_monthly_storage_cost())]
		#[pallet::call_index(3)]
		pub fn update_monthly_storage_cost(
			origin: OriginFor<T>,
			new_cost: BalanceOf<T>,
		) -> DispatchResult {
			ensure_root(origin)?;
			MonthlyStorageCost::<T>::put(new_cost);
			Ok(())
		}
    }
}
