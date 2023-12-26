#[derive(Debug, thiserror::Error)]
pub enum MerchantDBError {
    #[error("Error while encrypting DEK before adding to DB")]
    DEKEncryptionError,
    #[error("Error while decrypting DEK from DB")]
    DEKDecryptionError,
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding element in the database")]
    DBFilterError,
    #[error("Error while inserting element in the database")]
    DBInsertError,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum LockerDBError {
    #[error("Error while encrypting data before adding to DB")]
    DataEncryptionError,
    #[error("Error while decrypting data from DB")]
    DataDecryptionError,
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding element in the database")]
    DBFilterError,
    #[error("Error while inserting element in the database")]
    DBInsertError,
    #[error("Error while deleting element in the database")]
    DBDeleteError,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum HashDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding element in the database")]
    DBFilterError,
    #[error("Error while inserting element in the database")]
    DBInsertError,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum TestDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while writing to database")]
    DBWriteError,
    #[error("Error while reading element in the database")]
    DBReadError,
    #[error("Error while deleting element in the database")]
    DBDeleteError,
    #[error("Unpredictable error occurred")]
    UnknownError,
}
