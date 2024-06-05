#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency, Get},
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{CheckedAdd, CheckedSub, SaturatedConversion};
use sp_std::vec::Vec;
use sp_io::hashing::sha2_256;
use codec::{Encode, Decode, MaxEncodedLen};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_support::BoundedVec;

type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

fn convert_block_number_to_balance<T: Config>(block_number: BlockNumberFor<T>) -> BalanceOf<T> {
    // Convert block number to u128 and then to Balance
    let block_number_u128: u128 = block_number.saturated_into();
    block_number_u128.saturated_into()
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use scale_info::TypeInfo;
    use sp_std::prelude::*;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
        type TokenBalance: From<u64> + Into<u64> + Copy + CheckedAdd + CheckedSub + Default + TypeInfo + MaxEncodedLen + Clone;
        type WeightInfo: WeightInfo;
        type MiningReward: Get<BalanceOf<Self>>;
        type StorageRewardPeriod: Get<BlockNumberFor<Self>>;
        type MaxCIDLength: Get<u32> + Clone;
        type MaxProofDataLength: Get<u32> + Clone;
        type MaxRootHashLength: Get<u32> + Clone;
        type MaxHashLength: Get<u32> + Clone;
        type MaxProofHashesLength: Get<u32> + Clone;
    }

    type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Encode, Decode, Default, Clone, PartialEq, MaxEncodedLen, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct Proof<T: Config> {
        cid: BoundedVec<u8, T::MaxCIDLength>,
        storer: T::AccountId,
        verified: bool,
        proof_data: BoundedVec<u8, T::MaxProofDataLength>,
        timestamp: BlockNumberFor<T>,
        root_hash: BoundedVec<u8, T::MaxRootHashLength>,
        proof_hashes: BoundedVec<BoundedVec<u8, T::MaxHashLength>, T::MaxProofHashesLength>,
        reward_eligible: bool,
        storage_reward_per_block: BalanceOf<T>,
        pool_id: u32,
    }

    #[pallet::storage]
    #[pallet::getter(fn proofs)]
    pub type Proofs<T: Config> = StorageMap<_, Blake2_128Concat, BoundedVec<u8, T::MaxCIDLength>, Proof<T>>;

    #[pallet::storage]
    #[pallet::getter(fn replication_factors)]
    pub type ReplicationFactors<T: Config> = StorageMap<_, Blake2_128Concat, BoundedVec<u8, T::MaxCIDLength>, u32>;

    #[pallet::storage]
    #[pallet::getter(fn rewards)]
    pub type Rewards<T: Config> = StorageMap<_, Blake2_128Concat, BoundedVec<u8, T::MaxCIDLength>, BalanceOf<T>>;

    #[pallet::storage]
    #[pallet::getter(fn last_reward_distribution)]
    pub type LastRewardDistribution<T: Config> = StorageValue<_, BlockNumberFor<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProofSubmitted(Vec<u8>, T::AccountId),
        ProofVerified(Vec<u8>, T::AccountId, bool),
        RewardDistributed(T::AccountId, BalanceOf<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        ProofAlreadyExists,
        ProofNotFound,
        ReplicationFactorExhausted,
        VerificationFailed,
        InsufficientFunds,
        StorageRequestNotFound,
        IPFSInteractionFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(T::WeightInfo::submit_proof())]
        #[pallet::call_index(0)]
        pub fn submit_proof(
            origin: OriginFor<T>,
            cid: Vec<u8>,
            proof_data: Vec<u8>,
            root_hash: Vec<u8>,
            proof_hashes: Vec<Vec<u8>>,
            pool_id: u32,
        ) -> DispatchResult {
            let storer = ensure_signed(origin)?;

            let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone().try_into().map_err(|_| Error::<T>::ProofAlreadyExists)?;
            ensure!(!Proofs::<T>::contains_key(&bounded_cid), Error::<T>::ProofAlreadyExists);

            let bounded_proof_data: BoundedVec<u8, T::MaxProofDataLength> = proof_data.clone().try_into().map_err(|_| Error::<T>::ProofAlreadyExists)?;
            let bounded_root_hash: BoundedVec<u8, T::MaxRootHashLength> = root_hash.clone().try_into().map_err(|_| Error::<T>::ProofAlreadyExists)?;

            let bounded_proof_hashes_vec: Vec<BoundedVec<u8, T::MaxHashLength>> = proof_hashes
                .into_iter()
                .map(|h| h.try_into().map_err(|_| Error::<T>::ProofAlreadyExists))
                .collect::<Result<_, _>>()?;

            let bounded_proof_hashes: BoundedVec<BoundedVec<u8, T::MaxHashLength>, T::MaxProofHashesLength> =
                bounded_proof_hashes_vec.try_into().map_err(|_| Error::<T>::ProofAlreadyExists)?;

            let replication_factor = ReplicationFactors::<T>::get(&bounded_cid).ok_or(Error::<T>::ReplicationFactorExhausted)?;

            let reward_eligible = replication_factor > 0;
            let storage_reward_per_block: BalanceOf<T> = convert_block_number_to_balance::<T>(T::StorageRewardPeriod::get());

            let new_proof = Proof {
                cid: bounded_cid.clone(),
                storer: storer.clone(),
                verified: false,
                proof_data: bounded_proof_data.clone(),
                timestamp: <frame_system::Pallet<T>>::block_number(),
                root_hash: bounded_root_hash.clone(),
                proof_hashes: bounded_proof_hashes.clone(),
                reward_eligible,
                storage_reward_per_block,
                pool_id,
            };

            Proofs::<T>::insert(&bounded_cid, new_proof);
            ReplicationFactors::<T>::mutate(&bounded_cid, |rf| {
                if let Some(val) = rf {
                    *val = val.saturating_sub(1);
                }
            });

            Self::deposit_event(Event::ProofSubmitted(cid, storer));

            Ok(())
        }

        #[pallet::weight(T::WeightInfo::verify_proof())]
        #[pallet::call_index(1)]
        pub fn verify_proof(origin: OriginFor<T>, cid: Vec<u8>) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            let bounded_cid: BoundedVec<u8, T::MaxCIDLength> = cid.clone().try_into().map_err(|_| Error::<T>::ProofNotFound)?;
            let mut stored_proof = Proofs::<T>::get(&bounded_cid).ok_or(Error::<T>::ProofNotFound)?;

            // Off-chain IPFS interaction
            Self::offchain_verify_proof(cid.clone())?;

            stored_proof.verified = true;

            Proofs::<T>::insert(&bounded_cid, stored_proof.clone());

            Self::deposit_event(Event::ProofVerified(cid.clone(), stored_proof.storer.clone(), true));

            Ok(())
        }

        #[pallet::weight(T::WeightInfo::distribute_rewards())]
        #[pallet::call_index(2)]
        pub fn distribute_rewards(origin: OriginFor<T>) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            let current_block = <frame_system::Pallet<T>>::block_number();
            let last_distribution = LastRewardDistribution::<T>::get().unwrap_or_default();

            ensure!(current_block > last_distribution + T::StorageRewardPeriod::get(), "Distribution period has not elapsed");

            let mining_reward = T::MiningReward::get();
            for (_cid, proof) in Proofs::<T>::iter() {
                if proof.verified && proof.reward_eligible {
                    let storage_reward = proof.storage_reward_per_block;
                    let total_reward = storage_reward + mining_reward;
                    let _ = T::Currency::deposit_creating(&proof.storer, total_reward);
                    Self::deposit_event(Event::RewardDistributed(proof.storer.clone(), total_reward));
                }
            }

            LastRewardDistribution::<T>::put(current_block);

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn verify_merkle_proof(leaf_hash: &Vec<u8>, proof_hashes: &BoundedVec<BoundedVec<u8, T::MaxHashLength>, T::MaxProofHashesLength>, root_hash: &BoundedVec<u8, T::MaxRootHashLength>) -> bool {
            let mut current_hash = leaf_hash.clone();
            for proof_hash in proof_hashes.iter() {
                let mut combined = Vec::new();
                combined.extend(current_hash);
                combined.extend(proof_hash.clone());
                current_hash = sha2_256(&combined).to_vec();
            }
            current_hash == **root_hash
        }

        fn offchain_verify_proof(_cid: Vec<u8>) -> Result<(), Error<T>> {
            /*let mut stored_proof = Proofs::<T>::get(&bounded_cid).ok_or(Error::<T>::ProofNotFound)?;
            
            let ipfs_url = format!("http://localhost:5001/api/v0/cat?arg={}", sp_std::str::from_utf8(&cid).map_err(|_| Error::<T>::IPFSInteractionFailed)?);
            let response = http::Request::get(&ipfs_url).send().map_err(|_| Error::<T>::IPFSInteractionFailed)?;

            ensure!(response.code == 200, Error::<T>::IPFSInteractionFailed);
            let response_body = response.body().collect::<Vec<u8>>();

            let leaf_hash = sha2_256(&response_body).to_vec();
            ensure!(Self::verify_merkle_proof(&leaf_hash, &stored_proof.proof_hashes, &stored_proof.root_hash), Error::<T>::VerificationFailed);*/

            Ok(())
        }
    }
}
