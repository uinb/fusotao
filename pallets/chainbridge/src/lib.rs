#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
    ensure,
    traits::{EnsureOrigin, Get, StorageVersion},
    PalletId,
};
use fuso_support::{chainbridge::*, ChainId};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_runtime::traits::AccountIdConversion;
use sp_runtime::{traits::Dispatchable, RuntimeDebug};
use sp_std::prelude::*;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

const DEFAULT_RELAYER_THRESHOLD: u32 = 1;
const MODULE_ID: PalletId = PalletId(*b"oc/bridg");

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum ProposalStatus {
    Initiated,
    Approved,
    Rejected,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum BridgeEvent {
    FungibleTransfer(ChainId, DepositNonce, ResourceId, U256, Vec<u8>),
    NonFungibleTransfer(ChainId, DepositNonce, ResourceId, Vec<u8>, Vec<u8>, Vec<u8>),
    GenericTransfer(ChainId, DepositNonce, ResourceId, Vec<u8>),
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ProposalVotes<AccountId, BlockNumber> {
    pub votes_for: Vec<AccountId>,
    pub votes_against: Vec<AccountId>,
    pub status: ProposalStatus,
    pub expiry: BlockNumber,
}

impl<AccountId: PartialEq, BlockNumber: PartialOrd + Default>
    ProposalVotes<AccountId, BlockNumber>
{
    /// Attempts to mark the proposal as approve or rejected.
    /// Returns true if the status changes from active.
    fn try_to_complete(&mut self, threshold: u32, total: u32) -> ProposalStatus {
        if threshold as usize <= self.votes_for.len() {
            self.status = ProposalStatus::Approved;
            ProposalStatus::Approved
        } else if threshold <= total && total < self.votes_against.len() as u32 + threshold {
            self.status = ProposalStatus::Rejected;
            ProposalStatus::Rejected
        } else {
            ProposalStatus::Initiated
        }
    }

    /// Returns true if the proposal has been rejected or approved, otherwise false.
    fn is_complete(&self) -> bool {
        self.status != ProposalStatus::Initiated
    }

    /// Returns true if `who` has voted for or against the proposal
    fn hash_voted(&self, who: &AccountId) -> bool {
        self.votes_for.contains(&who) || self.votes_against.contains(&who)
    }

    /// Return true is the expiry time has been reached
    fn is_expired(&self, now: BlockNumber) -> bool {
        self.expiry <= now
    }
}

impl<AccountId, BlockNumber: Default> Default for ProposalVotes<AccountId, BlockNumber> {
    fn default() -> Self {
        Self {
            votes_for: sp_std::vec![],
            votes_against: sp_std::vec![],
            status: ProposalStatus::Initiated,
            expiry: BlockNumber::default(),
        }
    }
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[allow(dead_code)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use codec::EncodeLike;
    use frame_support::traits::fungibles::Mutate;
    use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;
    use fuso_support::traits::Token;

    type AssetId<T> =
        <<T as Config>::Fungibles as Token<<T as frame_system::Config>::AccountId>>::TokenId;

    type BalanceOf<T> =
        <<T as Config>::Fungibles as Token<<T as frame_system::Config>::AccountId>>::Balance;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Origin used to administer the pallet
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Proposed dispatchable call
        type Proposal: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
            + EncodeLike
            + GetDispatchInfo;

        /// This identifier for this chain.
        /// This must be unique and must not collide with existing IDs within a set of bridged
        /// chains.
        #[pallet::constant]
        type ChainId: Get<ChainId>;

        #[pallet::constant]
        type ProposalLifetime: Get<Self::BlockNumber>;

        #[pallet::constant]
        type TreasuryAccount: Get<Self::AccountId>;

        /// Expose customizable associated type of asset transfer, lock and unlock
        type Fungibles: Mutate<Self::AccountId, AssetId = AssetId<Self>, Balance = BalanceOf<Self>>
            + Token<Self::AccountId>;

        type NativeResourceId: Get<ResourceId>;

        /// Map of cross-chain asset ID & name
        type AssetIdByName: AssetIdResourceIdProvider<AssetId<Self>>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::without_storage_info]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// All whitelisted chains and their respective transaction counts
    #[pallet::storage]
    #[pallet::getter(fn chains)]
    pub type ChainNonces<T: Config> = StorageMap<_, Blake2_128Concat, ChainId, DepositNonce>;

    #[pallet::type_value]
    pub fn DefaultRelayerThreshold() -> u32 {
        DEFAULT_RELAYER_THRESHOLD
    }

    /// Number of votes required for a proposal to execute
    #[pallet::storage]
    #[pallet::getter(fn relayer_threshold)]
    pub type RelayerThreshold<T: Config> =
        StorageValue<_, u32, ValueQuery, DefaultRelayerThreshold>;

    /// Tracks current relayer set
    #[pallet::storage]
    #[pallet::getter(fn relayers)]
    pub type Relayers<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    /// Number of relayers in set
    #[pallet::storage]
    #[pallet::getter(fn relayer_count)]
    pub type RelayerCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// All known proposals.
    /// The key is the hash of the call and the deposit ID, to ensure it's unique.
    #[pallet::storage]
    #[pallet::getter(fn votes)]
    pub type Votes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        ChainId,
        Blake2_128Concat,
        (DepositNonce, T::Proposal),
        ProposalVotes<T::AccountId, T::BlockNumber>,
    >;

    /// Utilized by the bridge software to map resource IDs to actual methods
    #[pallet::storage]
    #[pallet::getter(fn resources)]
    pub type Resources<T: Config> = StorageMap<_, Blake2_128Concat, ResourceId, Vec<u8>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Vote threshold has changed \[new_threshold\]
        RelayerThresholdChanged(u32),
        /// Chain now available for transfers \[bridge_chain_id\]
        ChainWhitelisted(ChainId),
        /// Relayer added to set \[relayer_accout_id\]
        RelayerAdded(T::AccountId),
        /// Relayer removed from set \[relayer_accout_id\]
        RelayerRemoved(T::AccountId),
        /// FunglibleTransfer is for relaying fungibles \[dest_id, nonce, resource_id, amount,
        /// recipient, metadata\]
        FungibleTransfer(ChainId, DepositNonce, ResourceId, U256, Vec<u8>),
        /// NonFungibleTransfer is for relaying NFTS \[dest_id, nonce, resource_id, token_id,
        /// recipient, metadata\]
        NonFungibleTransfer(ChainId, DepositNonce, ResourceId, Vec<u8>, Vec<u8>, Vec<u8>),
        /// GenericTransfer is for a generic data payload \[dest_id, nonce, resource_id, metadata\]
        GenericTransfer(ChainId, DepositNonce, ResourceId, Vec<u8>),
        /// Vote submitted in favour of proposal \[bridge_chain_id, nonce, account_id\]
        VoteFor(ChainId, DepositNonce, T::AccountId),
        /// Vote submitted against proposal \[bridge_chain_id, nonce, account_id\]
        VoteAgainst(ChainId, DepositNonce, T::AccountId),
        /// Voting successful for a proposal \[bridge_chain_id, nonce\]
        ProposalApproved(ChainId, DepositNonce),
        /// Voting rejected a proposal \[bridge_chain_id, nonce\]
        ProposalRejected(ChainId, DepositNonce),
        /// Execution of call succeeded \[bridge_chain_id, nonce\]
        ProposalSucceeded(ChainId, DepositNonce),
        /// Execution of call failed \[bridge_chain_id, nonce\]
        ProposalFailed(ChainId, DepositNonce),
        ProposalVote(ChainId, EvmHash, DepositNonce),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        /// Relayer threshold not set
        ThresholdNotSet,
        /// Provide chain Id is not valid
        InvalidChainId,
        /// Relayer threshold cannot be 0
        InvalidThreshold,
        /// Interactions with this chai is not permitted
        ChainNotWhitelisted,
        /// Chain has already been enabled
        ChainAlreadyWhitelisted,
        /// Resource ID provided isn't mapped to anything
        ResourceDoesNotExist,
        /// Relayer already in set
        RelayerAlreadyExists,
        /// Provided accountId is not a relayer
        RelayerInvalid,
        /// Protected operation, must be performed by relayer
        MustBeRelayer,
        /// Relayer has already submitted some vote for this proposal
        RelayerAlreadyVoted,
        /// A proposal with these parameters has already been submitted
        ProposalAlreadyExists,
        /// No proposal with the ID was found
        ProposalDoesNotExist,
        /// Cannot complete proposal, needs more votes
        ProposalNotComplete,
        /// Proposal has either failed or succeeded
        ProposalAlreadyComplete,
        /// Lifetime of proposal has been exceeded
        ProposalExpired,
        /// TokenId not Found by ResourceId
        ResourceIdNotMapToToken,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Sets the vote threshold for proposals.
        ///
        /// This threshold is used to determine how many votes are required
        /// before a proposal is executed.
        ///
        /// # <weight>
        /// - O(1) lookup and insert
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn set_threshold(origin: OriginFor<T>, threshold: u32) -> DispatchResult {
            Self::ensure_admin(origin)?;
            Self::set_relayer_threshold(threshold)
        }

        /// Stores a method name on chain under an associated resource ID.
        ///
        /// # <weight>
        /// - O(1) write
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn set_resource(
            origin: OriginFor<T>,
            id: ResourceId,
            method: Vec<u8>,
        ) -> DispatchResult {
            Self::ensure_admin(origin)?;
            if id != T::NativeResourceId::get() {
                let (chain_id, _, contract) = decode_resource_id(id.clone());
                //check (chainId, contract) -> token mapping is ok
                T::AssetIdByName::try_get_asset_id(chain_id, contract)
                    .map_err(|_| Error::<T>::ResourceIdNotMapToToken)?;
            }
            Self::register_resource(id, method)
        }

        /// Removes a resource ID from the resource mapping.
        ///
        /// After this call, bridge transfers with the associated resource ID will
        /// be rejected.
        ///
        /// # <weight>
        /// - O(1) removal
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn remove_resource(origin: OriginFor<T>, id: ResourceId) -> DispatchResult {
            Self::ensure_admin(origin)?;
            Self::unregister_resource(id)
        }

        /// Enables a chain ID as a source or destination for a bridge transfer.
        ///
        /// # <weight>
        /// - O(1) lookup and insert
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn whitelist_chain(origin: OriginFor<T>, id: ChainId) -> DispatchResult {
            Self::ensure_admin(origin)?;
            Self::whitelist(id)
        }

        /// Adds a new relayer to the relayer set.
        ///
        /// # <weight>
        /// - O(1) lookup and insert
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn add_relayer(origin: OriginFor<T>, v: T::AccountId) -> DispatchResult {
            Self::ensure_admin(origin)?;
            Self::register_relayer(v)
        }

        /// Removes an existing relayer from the set.
        ///
        /// # <weight>
        /// - O(1) lookup and removal
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn remove_relayer(origin: OriginFor<T>, v: T::AccountId) -> DispatchResult {
            Self::ensure_admin(origin)?;
            Self::unregister_relayer(v)
        }

        /// Commits a vote in favour of the provided proposal.
        ///
        /// If a proposal with the given nonce and source chain ID does not already exist, it will
        /// be created with an initial vote in favour from the caller.
        ///
        /// # <weight>
        /// - weight of proposed call, regardless of whether execution is performed
        /// # </weight>
        #[pallet::weight({
            let dispatch_info = call.get_dispatch_info();
            (dispatch_info.weight.saturating_add(Weight::from_ref_time(195_000_000u64)), dispatch_info.class, Pays::Yes)
        })]
        pub fn acknowledge_proposal(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            r_id: ResourceId,
            evm_hash: EvmHash,
            call: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Self::is_relayer(&who), Error::<T>::MustBeRelayer);
            ensure!(
                Self::chain_whitelisted(src_id),
                Error::<T>::ChainNotWhitelisted
            );
            ensure!(
                Self::resource_exists(r_id),
                Error::<T>::ResourceDoesNotExist
            );
            Self::deposit_event(Event::ProposalVote(src_id, evm_hash, nonce));
            Self::vote_for(who, nonce, src_id, call)
        }

        /// Commits a vote against a provided proposal.
        ///
        /// # <weight>
        /// - Fixed, since execution of proposal should not be included
        /// # </weight>
        #[pallet::weight(195_000_0000)]
        pub fn reject_proposal(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            r_id: ResourceId,
            call: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Self::is_relayer(&who), Error::<T>::MustBeRelayer);
            ensure!(
                Self::chain_whitelisted(src_id),
                Error::<T>::ChainNotWhitelisted
            );
            ensure!(
                Self::resource_exists(r_id),
                Error::<T>::ResourceDoesNotExist
            );

            Self::vote_against(who, nonce, src_id, call)
        }

        /// Evaluate the state of a proposal given the current vote threshold.
        ///
        /// A proposal with enough votes will be either executed or cancelled, and the status
        /// will be updated accordingly.
        ///
        /// # <weight>
        /// - weight of proposed call, regardless of whether execution is performed
        /// # </weight>
        #[pallet::weight({
            let dispatch_info = prop.get_dispatch_info();
            (dispatch_info.weight.saturating_add(Weight::from_ref_time(195_000_000u64)), dispatch_info.class, Pays::Yes)
        })]
        pub fn eval_vote_state(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            ensure_signed(origin)?;

            Self::try_resolve_proposal(nonce, src_id, prop)
        }
    }

    impl<T: Config> Pallet<T> {
        // *** Utility methods ***

        pub fn ensure_admin(o: T::RuntimeOrigin) -> DispatchResult {
            T::AdminOrigin::ensure_origin(o)?;
            Ok(().into())
        }

        /// Checks if who is a relayer
        pub fn is_relayer(who: &T::AccountId) -> bool {
            Self::relayers(who)
        }

        /// Provides an AccountId for the pallet.
        /// This is used both as an origin check and deposit/withdrawal account.
        pub fn account_id() -> T::AccountId {
            MODULE_ID.try_into_account().unwrap()
        }

        /// Asserts if a resource is registered
        pub fn resource_exists(id: ResourceId) -> bool {
            Self::resources(id).is_some()
        }

        /// Checks if a chain exists as a whitelisted destination
        pub fn chain_whitelisted(id: ChainId) -> bool {
            Self::chains(id).is_some()
        }

        /// Increments the deposit nonce for the specified chain ID
        fn bump_nonce(id: ChainId) -> DepositNonce {
            let nonce = Self::chains(id).unwrap_or_default() + 1;
            <ChainNonces<T>>::insert(id, nonce);
            nonce
        }

        // *** Admin methods ***

        /// Set a new voting threshold
        pub fn set_relayer_threshold(threshold: u32) -> DispatchResult {
            ensure!(threshold > 0, <Error<T>>::InvalidThreshold);
            <RelayerThreshold<T>>::put(threshold);
            Self::deposit_event(Event::RelayerThresholdChanged(threshold));
            Ok(().into())
        }

        /// Register a method for a resource Id, enabling associated transfers
        pub fn register_resource(id: ResourceId, method: Vec<u8>) -> DispatchResult {
            <Resources<T>>::insert(id, method);
            Ok(().into())
        }

        /// Removes a resource ID, disabling associated transfer
        pub fn unregister_resource(id: ResourceId) -> DispatchResult {
            <Resources<T>>::remove(id);
            Ok(().into())
        }

        /// Whitelist a chain ID for transfer
        pub fn whitelist(id: ChainId) -> DispatchResult {
            // Cannot whitelist this chain
            ensure!(id != T::ChainId::get(), <Error<T>>::InvalidChainId);
            // Cannot whitelist with an existing entry
            ensure!(
                !Self::chain_whitelisted(id),
                <Error<T>>::ChainAlreadyWhitelisted
            );
            <ChainNonces<T>>::insert(&id, 0);
            Self::deposit_event(Event::ChainWhitelisted(id));
            Ok(().into())
        }

        /// Adds a new relayer to the set
        pub fn register_relayer(relayer: T::AccountId) -> DispatchResult {
            ensure!(
                !Self::is_relayer(&relayer),
                <Error<T>>::RelayerAlreadyExists
            );
            <Relayers<T>>::insert(&relayer, true);
            <RelayerCount<T>>::mutate(|i| *i += 1);
            Self::deposit_event(Event::RelayerAdded(relayer));
            Ok(().into())
        }

        /// Removes a relayer from the set
        pub fn unregister_relayer(relayer: T::AccountId) -> DispatchResult {
            ensure!(Self::is_relayer(&relayer), <Error<T>>::RelayerInvalid);
            <Relayers<T>>::remove(&relayer);
            <RelayerCount<T>>::mutate(|i| *i -= 1);
            Self::deposit_event(Event::RelayerRemoved(relayer));
            Ok(().into())
        }

        // ** Proposal voting and execution methods ***

        /// Commits a vote for a proposal. If the proposal doesn't exist it will be created.
        fn commit_vote(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
            in_favour: bool,
        ) -> DispatchResult {
            let now = <frame_system::Pallet<T>>::block_number();
            let mut votes = match <Votes<T>>::get(src_id, (nonce, prop.clone())) {
                Some(v) => v,
                None => {
                    let mut v = ProposalVotes::default();
                    v.expiry = now + T::ProposalLifetime::get();
                    v
                }
            };

            // Ensure the proposal isn't complete and relayer hasn't already voted
            ensure!(!votes.is_complete(), <Error<T>>::ProposalAlreadyComplete);
            ensure!(!votes.is_expired(now), <Error<T>>::ProposalExpired);
            ensure!(!votes.hash_voted(&who), <Error<T>>::RelayerAlreadyVoted);

            if in_favour {
                votes.votes_for.push(who.clone());
                Self::deposit_event(Event::VoteFor(src_id, nonce, who.clone()));
            } else {
                votes.votes_against.push(who.clone());
                Self::deposit_event(Event::VoteAgainst(src_id, nonce, who.clone()));
            }

            <Votes<T>>::insert(src_id, (nonce, prop.clone()), votes.clone());

            Ok(().into())
        }

        /// Attempts to finalize or cancel the proposal if the vote count allows.
        fn try_resolve_proposal(
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            if let Some(mut votes) = <Votes<T>>::get(src_id, (nonce, prop.clone())) {
                let now = <frame_system::Pallet<T>>::block_number();
                ensure!(!votes.is_complete(), <Error<T>>::ProposalAlreadyComplete);
                ensure!(!votes.is_expired(now), <Error<T>>::ProposalExpired);

                let status =
                    votes.try_to_complete(<RelayerThreshold<T>>::get(), <RelayerCount<T>>::get());
                <Votes<T>>::insert(src_id, (nonce, prop.clone()), votes.clone());

                match status {
                    ProposalStatus::Approved => Self::finalize_execution(src_id, nonce, prop),
                    ProposalStatus::Rejected => Self::cancel_execution(src_id, nonce),
                    _ => Ok(().into()),
                }
            } else {
                Err(<Error<T>>::ProposalDoesNotExist.into())
            }
        }

        /// Commits a vote in favour of the proposal and executes it if the vote threshold is met.
        fn vote_for(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            Self::commit_vote(who, nonce, src_id, prop.clone(), true)?;
            Self::try_resolve_proposal(nonce, src_id, prop)
        }

        /// Commits a vote against the proposal and cancels it if more than (relayers.len() -
        /// threshold) votes against exist.
        fn vote_against(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            Self::commit_vote(who, nonce, src_id, prop.clone(), false)?;
            Self::try_resolve_proposal(nonce, src_id, prop)
        }

        /// Execute the proposal and signals the result as an event
        fn finalize_execution(
            src_id: ChainId,
            nonce: DepositNonce,
            call: Box<T::Proposal>,
        ) -> DispatchResult {
            Self::deposit_event(<Event<T>>::ProposalApproved(src_id, nonce));

            call.dispatch(frame_system::RawOrigin::Signed(Self::account_id()).into())
                .map(|_| ())
                .map_err(|e| e.error)?;

            Self::deposit_event(Event::ProposalSucceeded(src_id, nonce));
            Ok(().into())
        }

        /// Cancels a proposal.
        fn cancel_execution(src_id: ChainId, nonce: DepositNonce) -> DispatchResult {
            Self::deposit_event(Event::ProposalRejected(src_id, nonce));
            Ok(().into())
        }

        /// Initiates a transfer of a fungible asset out of the chain. This should be called by
        /// another pallet.
        pub fn transfer_fungible(
            dest_id: ChainId,
            resource_id: ResourceId,
            to: Vec<u8>,
            amount: U256,
        ) -> DispatchResult {
            ensure!(
                Self::chain_whitelisted(dest_id),
                <Error<T>>::ChainNotWhitelisted
            );
            let nonce = Self::bump_nonce(dest_id);
            Self::deposit_event(Event::FungibleTransfer(
                dest_id,
                nonce,
                resource_id,
                amount,
                to,
            ));

            Ok(().into())
        }

        /// Initiates a transfer of a nonfungible asset out of the chain. This should be called by
        /// another pallet.
        pub fn transfer_nonfungible(
            dest_id: ChainId,
            resource_id: ResourceId,
            token_id: Vec<u8>,
            to: Vec<u8>,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            ensure!(
                Self::chain_whitelisted(dest_id),
                <Error<T>>::ChainNotWhitelisted
            );
            let nonce = Self::bump_nonce(dest_id);
            Self::deposit_event(Event::NonFungibleTransfer(
                dest_id,
                nonce,
                resource_id,
                token_id,
                to,
                metadata,
            ));

            Ok(().into())
        }

        /// Initiates a transfer of generic data out of the chain. This should be called by another
        /// pallet.
        pub fn transfer_generic(
            dest_id: ChainId,
            resource_id: ResourceId,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            ensure!(
                Self::chain_whitelisted(dest_id),
                <Error<T>>::ChainNotWhitelisted
            );
            let nonce = Self::bump_nonce(dest_id);

            Self::deposit_event(Event::GenericTransfer(
                dest_id,
                nonce,
                resource_id,
                metadata,
            ));
            Ok(().into())
        }
    }
}

/// Simple ensure origin for the bridge account
pub struct EnsureBridge<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> EnsureOrigin<T::RuntimeOrigin> for EnsureBridge<T> {
    type Success = T::AccountId;

    fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
        let bridge_id = MODULE_ID.try_into_account().unwrap();
        o.into().and_then(|o| match o {
            frame_system::RawOrigin::Signed(who) if who == bridge_id => Ok(bridge_id),
            r => Err(T::RuntimeOrigin::from(r)),
        })
    }

    /// Returns an outer origin capable of passing `try_origin` check.
    ///
    /// ** Should be used for benchmarking only!!! **
    #[cfg(feature = "runtime-benchmarks")]
    fn successful_origin() -> T::RuntimeOrigin {
        T::RuntimeOrigin::from(frame_system::RawOrigin::Signed(<Pallet<T>>::account_id()))
    }
}
