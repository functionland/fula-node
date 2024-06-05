#![cfg_attr(not(feature = "std"), no_std)]
// pool-pallet

pub use pallet::*;

use frame_support::{
    pallet_prelude::*,
    traits::Get,
    BoundedVec,
};
use frame_system::ensure_signed;
use codec::{Encode, Decode};
use sp_std::vec::Vec;
use core::cmp;
use sp_std::prelude::*;
use sp_std::fmt::Debug;
use codec::EncodeLike;
use frame_system::pallet_prelude::BlockNumberFor;


pub trait PoolInterface {
    type AccountId;
    type PoolId: Copy + TypeInfo + Debug + Eq + EncodeLike + Encode + Decode;
    fn is_member(account: Self::AccountId, pool: Self::PoolId) -> bool;
}

/// Type used for a unique identifier of each pool.
type PoolId = u32;

/// An enum that represents a vote result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VoteResult {
    /// Majority voted for.
    Accepted,
    /// Majority voted against.
    Denied,
    /// Not conclusive yet.
    Inconclusive,
}

pub type BoundedStringOf<T> = BoundedVec<u8, <T as Config>::StringLimit>;

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    pub name: BoundedVec<u8, T::StringLimit>,
    pub owner: Option<T::AccountId>,
    pub parent: Option<PoolId>,
    pub participants: BoundedVec<T::AccountId, T::MaxPoolParticipants>,
    pub request_number: u8,
    pub region: BoundedVec<u8, T::StringLimit>,
}

impl<T: Config> Pool<T> {
    pub fn is_full(&self) -> bool {
        self.participants.len() + self.request_number as usize == T::MaxPoolParticipants::get() as usize
    }
}

#[derive(Clone, Encode, Decode, Default, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct User<BoundedString> {
    pub pool_id: Option<PoolId>,
    pub request_pool_id: Option<PoolId>,
    pub peer_id: BoundedString,
}

impl<BoundedString> User<BoundedString> {
    pub(crate) fn is_free(&self) -> bool {
        self.pool_id.is_none() && self.request_pool_id.is_none()
    }
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct PoolRequest<T: Config> {
    pub voted: BoundedVec<T::AccountId, T::MaxPoolParticipants>,
    pub positive_votes: u16,
    pub peer_id: BoundedVec<u8, T::StringLimit>,
}

impl<T: Config> Default for PoolRequest<T> {
    fn default() -> Self {
        PoolRequest {
            positive_votes: Default::default(),
            voted: Default::default(),
            peer_id: Default::default(),
        }
    }
}

impl<T: Config> Default for Pool<T> {
    fn default() -> Self {
        Pool {
            name: BoundedVec::default(),
            owner: None,
            parent: None,
            participants: BoundedVec::default(),
            request_number: 0,
            region: BoundedVec::default(),
        }
    }
}

impl<T: Config> PoolRequest<T> {
    pub(crate) fn check_votes(&self, num_participants: u16, pool_id: PoolId) -> VoteResult {
        let pool = Pools::<T>::get(pool_id).unwrap_or_default();
        for voter in &self.voted {
            if let Some(ref owner) = pool.owner {
                if *voter == *owner && self.positive_votes >= 1 {
                    return VoteResult::Accepted;
                }
            }
        }
        let min_votes_required = cmp::min(num_participants / 3, 8);
        if self.voted.len() as u16 - self.positive_votes > num_participants / 2 {
            return VoteResult::Denied;
        }
        if self.positive_votes > min_votes_required {
            return VoteResult::Accepted;
        }
        VoteResult::Inconclusive
    }
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
        type WeightInfo: WeightInfo;
        #[pallet::constant]
        type StringLimit: Get<u32>;
        #[pallet::constant]
        type MaxPoolParticipants: Get<u32>;
    }

    #[pallet::storage]
    pub type LastPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

    #[pallet::storage]
    pub type MaxPools<T: Config> = StorageValue<_, PoolId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pool)]
    pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, Pool<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn request)]
    pub type PoolRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PoolId,
        Blake2_128Concat,
        T::AccountId,
        PoolRequest<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn user)]
    pub type Users<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, User<BoundedStringOf<T>>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoolCreated {
            owner: Option<T::AccountId>,
            pool_id: PoolId,
        },
        JoinRequested {
            account: T::AccountId,
            pool_id: PoolId,
        },
        RequestWithdrawn {
            account: T::AccountId,
            pool_id: PoolId,
        },
        VotingResult {
            account: T::AccountId,
            pool_id: PoolId,
            result: Vec<u8>,
        },
        CapacityReached {
            pool_id: PoolId,
        },
        ParticipantLeft {
            account: T::AccountId,
            pool_id: PoolId,
        },
    }

    #[pallet::error]
    #[cfg_attr(test, derive(PartialEq))]
    pub enum Error<T> {
        UserBusy,
        MaxPools,
        NameTooLong,
        PoolDoesNotExist,
        RequestDoesNotExist,
        CapacityReached,
        UserDoesNotExist,
        AccessDenied,
        InternalError,
        AlreadyVoted,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(T::WeightInfo::create())]
        #[pallet::call_index(0)]
        pub fn create(
            origin: OriginFor<T>,
            name: Vec<u8>,
            region: Vec<u8>,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let mut user = Self::get_or_create_user(&owner);

            ensure!(user.is_free(), Error::<T>::UserBusy);

            if let Some(max_pools) = MaxPools::<T>::get() {
                ensure!(max_pools > LastPoolId::<T>::get(), Error::<T>::MaxPools);
            }

            let pool_id = LastPoolId::<T>::mutate(|id| {
                *id += 1;
                *id
            });

            let bounded_name: BoundedVec<u8, T::StringLimit> = name
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;

            let bounded_region: BoundedVec<u8, T::StringLimit> = region
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;

            let mut bounded_participants =
                BoundedVec::<T::AccountId, T::MaxPoolParticipants>::default();

            ensure!(
                bounded_participants.try_push(owner.clone()).is_ok(),
                Error::<T>::CapacityReached
            );

            let pool = Pool {
                name: bounded_name,
                region: bounded_region,
                owner: Some(owner.clone()),
                parent: None,
                participants: bounded_participants,
                request_number: 0,
            };
            Pools::<T>::insert(pool_id.clone(), pool);
            user.pool_id = Some(pool_id.clone());
            user.peer_id = peer_id;
            Users::<T>::set(&owner, Some(user));

            Self::deposit_event(Event::<T>::PoolCreated {
                pool_id,
                owner: Some(owner),
            });

            Ok(())
        }

        #[pallet::weight(T::WeightInfo::leave_pool())]
        #[pallet::call_index(1)]
        pub fn leave_pool(
            origin: OriginFor<T>, 
            pool_id: PoolId,
            target_account: Option<T::AccountId>, 
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            let account_to_remove = match target_account {
                Some(account) if Some(&caller) == pool.owner.as_ref() || caller == account => {
                    account
                },
                None => {
                    ensure!(pool.owner.is_none() || pool.owner == Some(caller.clone()), Error::<T>::AccessDenied);
                    caller
                },
                _ => return Err(Error::<T>::AccessDenied.into()),
            };

            let mut user = Users::<T>::get(&account_to_remove).ok_or(Error::<T>::UserDoesNotExist)?;
            ensure!(
                user.pool_id.is_some() && pool_id == user.pool_id.unwrap(),
                Error::<T>::AccessDenied
            );

            let mut pool: Pool<T> = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;
            let mut participants = pool.participants.clone();

            match participants.binary_search(&account_to_remove) {
                Ok(index) => {
                    participants.remove(index);
                    pool.participants = participants;
                    Pools::<T>::set(&pool_id, Some(pool));

                    user.pool_id = None;
                    Users::<T>::set(&account_to_remove, Some(user));

                    Self::deposit_event(Event::<T>::ParticipantLeft { pool_id, account: account_to_remove });
                    Ok(())
                }
                Err(_) => {
                    frame_support::defensive!(
                        "a user is not a participant of the pool they are assigned to"
                    );
                    Err(Error::<T>::InternalError.into())
                }
            }
        }

        #[pallet::weight(T::WeightInfo::join())]
        #[pallet::call_index(2)]
        pub fn join(
            origin: OriginFor<T>,
            pool_id: PoolId,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let mut pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            ensure!(!pool.is_full(), Error::<T>::CapacityReached);

            let mut user = Self::get_or_create_user(&account);

            ensure!(user.is_free(), Error::<T>::UserBusy);

            user.request_pool_id = Some(pool_id);
            user.peer_id = peer_id.clone();
            Users::<T>::set(&account, Some(user));

            let mut request = PoolRequest::<T>::default();
            request.peer_id = peer_id;
            PoolRequests::<T>::insert(&pool_id, &account, request);
            pool.request_number += 1;
            Pools::<T>::set(&pool_id, Some(pool));

            Self::deposit_event(Event::<T>::JoinRequested { pool_id, account });
            Ok(())
        }

        #[pallet::weight(T::WeightInfo::cancel_join())]
        #[pallet::call_index(3)]
        pub fn cancel_join(
            origin: OriginFor<T>, 
            pool_id: PoolId, 
            target_account: Option<T::AccountId>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            let pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            let account_to_cancel = match target_account {
                Some(account) if Some(&caller) == pool.owner.as_ref() => {
                    account
                },
                None => {
                    caller.clone()
                },
                _ => return Err(Error::<T>::AccessDenied.into()),
            };
            
            Self::request(&pool_id, &account_to_cancel).ok_or(Error::<T>::RequestDoesNotExist)?;
            let mut user = Self::user(&account_to_cancel).ok_or(Error::<T>::UserDoesNotExist)?;
            let pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            user.request_pool_id = None;
            Users::<T>::set(&account_to_cancel, Some(user));

            PoolRequests::<T>::remove(&pool_id, &account_to_cancel);

            Self::remove_pool_request(&account_to_cancel, pool_id, pool);

            Self::deposit_event(Event::<T>::RequestWithdrawn { 
                pool_id, 
                account: account_to_cancel, 
            });
            
            Ok(())
        }

        #[pallet::weight(T::WeightInfo::vote())]
        #[pallet::call_index(4)]
        pub fn vote(
            origin: OriginFor<T>,
            pool_id: PoolId,
            account: T::AccountId,
            positive: bool,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            let voter_user = Self::get_user(&voter)?;

            let mut request = Self::request(&pool_id, &account).ok_or(Error::<T>::RequestDoesNotExist)?;

            ensure!(voter_user.pool_id.is_some() && voter_user.pool_id.unwrap() == pool_id, Error::<T>::AccessDenied);
            ensure!(request.peer_id.to_vec() == peer_id.to_vec(), Error::<T>::AccessDenied);

            let mut voted = request.voted.clone();

            match voted.binary_search(&voter) {
                Ok(_) => Err(Error::<T>::AlreadyVoted.into()),
                Err(index) => {
                    voted.try_insert(index, voter.clone()).map_err(|_| Error::<T>::InternalError)?;
                    if positive {
                        request.positive_votes += 1;
                    }
                    request.voted = voted;

                    let pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;
                    ensure!(!pool.is_full(), Error::<T>::CapacityReached);

                    let result = request.check_votes(pool.participants.len() as u16, pool_id);
                    Self::process_vote_result(&account, pool_id, pool, request, result)
                }
            }
        }

        #[pallet::weight(T::WeightInfo::get_pool_members())]
        #[pallet::call_index(5)]
        pub fn get_pool_members(
            origin: OriginFor<T>,
            pool_id: Option<PoolId>,
            account_id: Option<T::AccountId>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            let mut members: Vec<(T::AccountId, BoundedVec<u8, T::StringLimit>, PoolId)> = Vec::new();

            for (user_id, user) in Users::<T>::iter() {
                if let Some(pid) = pool_id {
                    if user.pool_id == Some(pid) {
                        members.push((user_id.clone(), user.peer_id.clone(), pid));
                    }
                } else if let Some(ref aid) = account_id {
                    if user_id == *aid {
                        if let Some(pid) = user.pool_id {
                            members.push((user_id.clone(), user.peer_id.clone(), pid));
                        }
                    }
                }
            }

            // Return the members list
            // log::info!("Pool members: {:?}", members);

            Ok(())
        }

        #[pallet::weight(T::WeightInfo::list_pools())]
        #[pallet::call_index(6)]
        pub fn list_pools(
            origin: OriginFor<T>,
            pool_id: Option<PoolId>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            let mut pools: Vec<(PoolId, BoundedVec<u8, T::StringLimit>, BoundedVec<u8, T::StringLimit>, BlockNumberFor<T>, Option<T::AccountId>, Option<BoundedVec<u8, T::StringLimit>>, Vec<(T::AccountId, BoundedVec<u8, T::StringLimit>)>)> = Vec::new();

            for (pid, pool) in Pools::<T>::iter() {
                if let Some(id) = pool_id {
                    if pid == id {
                        let mut members: Vec<(T::AccountId, BoundedVec<u8, T::StringLimit>)> = Vec::new();
                        for member in pool.participants.iter() {
                            if let Some(user) = Users::<T>::get(member) {
                                members.push((member.clone(), user.peer_id.clone()));
                            }
                        }
                        pools.push((pid, pool.name.clone(), pool.region.clone(), frame_system::Pallet::<T>::block_number(), pool.owner.clone(), pool.owner.as_ref().map(|owner| Users::<T>::get(owner).unwrap().peer_id.clone()), members));
                    }
                } else {
                    let mut members: Vec<(T::AccountId, BoundedVec<u8, T::StringLimit>)> = Vec::new();
                    for member in pool.participants.iter() {
                        if let Some(user) = Users::<T>::get(member) {
                            members.push((member.clone(), user.peer_id.clone()));
                        }
                    }
                    pools.push((pid, pool.name.clone(), pool.region.clone(), frame_system::Pallet::<T>::block_number(), pool.owner.clone(), pool.owner.as_ref().map(|owner| Users::<T>::get(owner).unwrap().peer_id.clone()), members));
                }
            }

            // Return the pools list
            // log::info!("Pools: {:?}", pools);

            Ok(())
        }


        #[pallet::weight(T::WeightInfo::list_pool_join_requests())]
        #[pallet::call_index(7)]
        pub fn list_pool_join_requests(
            origin: OriginFor<T>,
            pool_id: Option<PoolId>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            let mut requests: Vec<(T::AccountId, BoundedVec<u8, T::StringLimit>, BlockNumberFor<T>)> = Vec::new();

            // Correctly destructuring the iterator
            for (pid, user_id, request) in PoolRequests::<T>::iter().map(|(pid, user_id, request)| (pid, user_id, request)) {
                if let Some(id) = pool_id {
                    if pid == id {
                        requests.push((user_id.clone(), request.peer_id.clone(), frame_system::Pallet::<T>::block_number()));
                    }
                } else {
                    requests.push((user_id.clone(), request.peer_id.clone(), frame_system::Pallet::<T>::block_number()));
                }
            }

            // Return the join requests list
            // log::info!("Pool join requests: {:?}", requests);

            Ok(())
        }

    }

    impl<T: Config> Pallet<T> {
        fn get_or_create_user(who: &T::AccountId) -> User<BoundedStringOf<T>> {
            if let Some(user) = Self::user(who) {
                return user;
            }
            let user = User::default();

            Users::<T>::insert(who, user.clone());

            user
        }

        fn get_user(who: &T::AccountId) -> Result<User<BoundedStringOf<T>>, DispatchError> {
            Self::user(who).ok_or(Error::<T>::UserDoesNotExist.into())
        }

        fn remove_pool_request(who: &T::AccountId, pool_id: PoolId, mut pool: Pool<T>) {
            PoolRequests::<T>::remove(pool_id, who);
            pool.request_number -= 1;
            Pools::<T>::set(&pool_id, Some(pool));
        }

        fn process_vote_result(
            who: &T::AccountId,
            pool_id: PoolId,
            mut pool: Pool<T>,
            request: PoolRequest<T>,
            result: VoteResult,
        ) -> DispatchResult {
            match result {
                VoteResult::Accepted => {
                    let mut user = Self::get_user(who)?;
                    PoolRequests::<T>::remove(pool_id, who);
                    let mut participants = pool.participants.clone();
                    match participants.binary_search(who) {
                        Ok(_) => Err(Error::<T>::InternalError.into()),
                        Err(index) => {
                            participants.try_insert(index, who.clone()).map_err(|_| Error::<T>::InternalError)?;
                            user.pool_id = Some(pool_id.clone());
                            user.request_pool_id = None;
                            user.peer_id = request.peer_id.into();
                            Users::<T>::set(who, Some(user));

                            pool.participants = participants;
                            Self::remove_pool_request(who, pool_id, pool);
                            let result = "Accepted";

                            Self::deposit_event(Event::<T>::VotingResult {
                                pool_id,
                                account: who.clone(),
                                result: result.as_bytes().to_vec(),
                            });
                            Ok(())
                        }
                    }
                }
                VoteResult::Denied => {
                    let mut user = Self::get_user(who)?;
                    user.request_pool_id = None;
                    Users::<T>::set(who, Some(user));

                    Self::remove_pool_request(who, pool_id, pool);
                    let result = "Denied";

                    Self::deposit_event(Event::<T>::VotingResult {
                        pool_id,
                        account: who.clone(),
                        result: result.as_bytes().to_vec(),
                    });

                    Ok(())
                }
                VoteResult::Inconclusive => {
                    PoolRequests::<T>::set(&pool_id, who, Some(request));
                    let result = "Inconclusive";
                    Self::deposit_event(Event::<T>::VotingResult {
                        pool_id,
                        account: who.clone(),
                        result: result.as_bytes().to_vec(),
                    });
                    Ok(())
                }
            }
        }
    }

    impl<T: Config> PoolInterface for Pallet<T> {
        type AccountId = T::AccountId;
        type PoolId = PoolId;

        fn is_member(account: Self::AccountId, pool: Self::PoolId) -> bool {
            Self::user(&account)
                .map(|u| u.pool_id.iter().any(|&v| v == pool))
                .unwrap_or(false)
        }
    }
}
