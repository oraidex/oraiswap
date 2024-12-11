use cosmwasm_std::{Decimal, StdError, StdResult, Uint128};

pub trait Converter128 {
    fn checked_div_decimal(&self, denominator: Decimal) -> StdResult<Self>
    where
        Self: Sized;
}

pub trait DecimalPlaces {
    fn limit_decimal_places(&self, maximum_decimal_places: Option<u32>) -> StdResult<Self>
    where
        Self: Sized;
}

impl Converter128 for Uint128 {
    fn checked_div_decimal(&self, denominator: Decimal) -> StdResult<Uint128> {
        Decimal::one()
            .checked_div(denominator)
            .map_err(|err| StdError::generic_err(err.to_string()))
            .map(|coeff| *self * coeff)
    }
}

pub const DEFAULT_MAX_DECIMAL_PLACES: u32 = 6;
impl DecimalPlaces for Decimal {
    fn limit_decimal_places(&self, _maximum_decimal_places: Option<u32>) -> StdResult<Self>
    where
        Self: Sized,
    {
        let mut maximum_decimal_places =
            _maximum_decimal_places.unwrap_or(DEFAULT_MAX_DECIMAL_PLACES);
        if maximum_decimal_places > DEFAULT_MAX_DECIMAL_PLACES {
            maximum_decimal_places = DEFAULT_MAX_DECIMAL_PLACES;
        }
        let numerator = 10u32.pow(maximum_decimal_places);
        let denominator = 1u32;

        (self.checked_mul(Decimal::from_ratio(numerator, denominator)))?
            .floor()
            .checked_div(Decimal::from_ratio(numerator, denominator))
            .map_err(|err| StdError::generic_err(err.to_string()))
    }
}
