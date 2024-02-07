use crate::storage::consts;

pub fn generate_id(id_length: usize) -> String {
    nanoid::nanoid!(id_length, &consts::ALPHABETS)
}