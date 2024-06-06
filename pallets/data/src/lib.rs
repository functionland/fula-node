#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod types;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency, Get},
    BoundedVec,
};
use frame_system::ensure_signed;
use codec::{Encode, Decode, EncodeLike};
use sp_std::vec::Vec;
use sp_std::prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::fmt::Debug;
use types::BoundedCIDList;

pub trait DataInterface {
    type AccountId;
    type PoolId: Copy + TypeInfo + Debug + Eq + EncodeLike + Encode + Decode;
    fn get_cids_by_pool(pool_id: Self::PoolId) -> Vec<Vec<u8>>;
}

type PoolId = u32;
pub type BoundedStringOf<T> = BoundedVec<u8, <T as Config>::StringLimit>;

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct CIDInfo<T: Config> {
    pub cid: BoundedVec<u8, T::MaxCIDLength>,
    pub uploader: T::AccountId,
    pub pool_id: PoolId,
    pub timestamp: BlockNumberFor<T>,
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::BoundedVec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
        type MaxCIDLength: Get<u32> + TypeInfo;
        type StringLimit: Get<u32>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    #[pallet::getter(fn cids)]
    pub type CIDs<T: Config> = StorageMap<_, Blake2_128Concat, BoundedVec<u8, T::MaxCIDLength>, CIDInfo<T>>;

    #[pallet::storage]
    #[pallet::getter(fn cids_by_pool)]
    pub type CIDsByPool<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, BoundedCIDList<T::MaxCIDLength>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CIDUploaded(T::AccountId, Vec<Vec<u8>>, PoolId),
    }

    #[pallet::error]
    pub enum Error<T> {
        CIDAlreadyExists,
        InvalidPoolID,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(T::WeightInfo::upload_cids())]
        #[pallet::call_index(0)]
        pub fn upload_cids(
            origin: OriginFor<T>,
            cids: Vec<Vec<u8>>,
            pool_id: PoolId,
        ) -> DispatchResult {
            let uploader = ensure_signed(origin)?;

            for cid in cids.iter() {
                let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone().try_into().map_err(|_| Error::<T>::CIDAlreadyExists)?;
                ensure!(!CIDs::<T>::contains_key(&bounded_cid), Error::<T>::CIDAlreadyExists);

                let new_cid_info = CIDInfo {
                    cid: bounded_cid.clone(),
                    uploader: uploader.clone(),
                    pool_id,
                    timestamp: <frame_system::Pallet<T>>::block_number(),
                };

                CIDs::<T>::insert(&bounded_cid, new_cid_info);

                CIDsByPool::<T>::mutate(pool_id, |cids| {
                    if let Some(cid_list) = cids {
                        if cid_list.0.try_push(bounded_cid.clone()).is_err() {
                            return Err(Error::<T>::CIDAlreadyExists);
                        }
                    } else {
                        let new_list = vec![bounded_cid.clone()];
                        *cids = Some(BoundedCIDList::from(new_list));
                    }
                    Ok(())
                })?;
            }

            Self::deposit_event(Event::CIDUploaded(uploader, cids, pool_id));

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn get_cids_by_pool(pool_id: PoolId) -> Vec<Vec<u8>> {
            if let Some(cids) = CIDsByPool::<T>::get(pool_id) {
                cids.0.into_iter().map(|cid| cid.to_vec()).collect()
            } else {
                Vec::new()
            }
        }
    }
}
