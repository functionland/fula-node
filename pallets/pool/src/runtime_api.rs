// runtime-api.rs
use codec::{Codec, Decode, Encode};
use  sp_std::vec::Vec;
use sp_api;

sp_api::decl_runtime_apis! {
    pub trait PoolApi<AccountId, PoolId, BoundedString> where
        AccountId: Codec,
        PoolId: Codec,
        BoundedString: Codec,
    {
        fn get_pool_members(pool_id: Option<PoolId>, account_id: Option<AccountId>) -> Vec<(AccountId, BoundedString, PoolId)>;
        fn list_pools(pool_id: Option<PoolId>) -> Vec<(PoolId, BoundedString, BoundedString, u32, Option<AccountId>, Option<BoundedString>, Vec<(AccountId, BoundedString)>)>;
        fn list_pool_join_requests(pool_id: Option<PoolId>) -> Vec<(AccountId, BoundedString, u32)>;
    }
}
