use solana_program::{
    keccak,
    secp256k1_recover::{secp256k1_recover, Secp256k1RecoverError},
};
use primitives::alloy_primitives::B512;
use primitives::B256;


/// Recover the public key from a signature and a message.
///
/// Solana onchain adapted implementation using syscall
pub fn ecrecover(sig: &B512, recid: u8, msg: &B256) -> Result<B256, Secp256k1RecoverError> {
    let public = secp256k1_recover(msg.as_slice(), recid, sig.as_slice())?;

    let mut hash = keccak::hash(&public.to_bytes()).to_bytes();
    hash[..12].fill(0);
    Ok(B256::from_slice(&hash))
}
