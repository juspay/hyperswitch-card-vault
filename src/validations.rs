///
/// Maximum limit of a card number will not exceed 19 by ISO standards
///
const CARD_NUMBER_LENGTH: usize = 20;

#[allow(clippy::expect_used)]
///
/// # Panics
///
/// Never, as a single character will never be greater than 10, or `u8`
///
pub fn luhn_on_string(number: &str) -> bool {
    let data: Vec<u8> = number
        .chars()
        .filter_map(|value| value.to_digit(10))
        .map(|value| {
            value
                .try_into()
                .expect("error while converting a single character to u8")
        }) // safety, a single character will never be greater
        // `u8`
        .collect();

    (data.len() < CARD_NUMBER_LENGTH)
        .then(|| luhn(&data))
        .unwrap_or(false)
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
