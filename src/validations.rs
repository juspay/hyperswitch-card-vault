use crate::error::ApiError;

const MIN_CARD_NUMBER_LENGTH: usize = 12;

///
/// Maximum limit of a card number will not exceed 19 by ISO standards
///
const MAX_CARD_NUMBER_LENGTH: usize = 19;

#[allow(clippy::expect_used)]
///
/// # Panics
///
/// Never, as a single character will never be greater than 10, or `u8`
///
pub fn luhn_on_string(number: &str) -> Result<bool, ApiError> {
    let number = number.split_whitespace().collect::<String>();

    let data = number
        .chars()
        .try_fold(Vec::with_capacity(20), |mut data, character| {
            data.push(
                character
                    .to_digit(10)
                    .ok_or(ApiError::ValidationError(
                        "invalid character found in card number",
                    ))?
                    .try_into()
                    .expect("error while converting a single character to u8"), // safety, a single character will never be greater `u8`
            );
            Ok::<Vec<u8>, ApiError>(data)
        })?;

    Ok(
        (data.len() >= MIN_CARD_NUMBER_LENGTH && data.len() <= MAX_CARD_NUMBER_LENGTH)
            .then(|| luhn(&data))
            .unwrap_or(false),
    )
}

#[allow(clippy::as_conversions)]
pub fn luhn(number: &[u8]) -> bool {
    number
        .iter()
        .rev()
        .enumerate()
        .map(|(idx, element)| {
            ((*element * 2) / 10 + (*element * 2) % 10) * ((idx as u8) % 2)
                + (*element) * (((idx + 1) as u8) % 2)
        })
        .sum::<u8>()
        % 10
        == 0
}
