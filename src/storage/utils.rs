use crate::storage::consts;

pub fn generate_id() -> String {
    nanoid::nanoid!(20, &consts::ALPHABETS)
}
