use codec::{Encode, Decode, MaxEncodedLen};
use frame_support::BoundedVec;
use sp_std::vec::Vec;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_runtime::traits::Get;

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct BoundedCIDList<T: Get<u32> + TypeInfo>(pub BoundedVec<BoundedVec<u8, T>, T>);

impl<T: Get<u32> + TypeInfo> From<Vec<BoundedVec<u8, T>>> for BoundedCIDList<T> {
    fn from(vec: Vec<BoundedVec<u8, T>>) -> Self {
        BoundedCIDList(BoundedVec::try_from(vec).expect("Vec to BoundedVec conversion should succeed"))
    }
}

impl<T: Get<u32> + TypeInfo> Into<Vec<BoundedVec<u8, T>>> for BoundedCIDList<T> {
    fn into(self) -> Vec<BoundedVec<u8, T>> {
        self.0.into_inner()
    }
}
