use rand::rngs::OsRng;
use rand::{distributions::Alphanumeric, Rng};

pub fn generate_token() -> String {
    OsRng
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}
