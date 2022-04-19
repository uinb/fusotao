# Description of Fusotao Pallets

All the pallets from [Fusotao Protocol](https://github.com/uinb/fusotao-protocol) are under audit by the [SlowMist](https://www.slowmist.com/).

## [pallet-fuso-verifier](https://github.com/uinb/fusotao-protocol)

This pallet provides a set of on-chain extrinsics to manage the off-chain matchers and verify the proofs submitted by them, as well as to handle the users' authorization & revoking requests.

## [pallet-fuso-token](https://github.com/uinb/fusotao-protocol)

This pallet is a wrapper of [pallet-balances](https://github.com/parity/substrate) and supports other wrapper tokens migrating from NEAR to be reserved like native token.

## [pallet-fuso-foundation](https://github.com/uinb/fusotao-protocol)

This pallet doesn't contain any extrinsics but a hook to determine when to unlock the pre-funded tokens accoding to the genesis config.

## [pallet-fuso-reward](https://github.com/uinb/fusotao-protocol)

The logic of calculating users' rewards, dependent by the [pallet-fuso-verifier](#pallet-fuso-verifier).

## [pallet-multisig](https://github.com/parity/substrate)

To enable multi-signature.
