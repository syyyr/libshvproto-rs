//use crate::rpcvalue::RpcValue;

/// mantisa: 56, exponent: 8;
/// I'm storing whole Decimal in one i64 to keep size_of RpcValue == 24
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Decimal {
    mantissa: i64,
    exponent: i8,
}

impl Decimal {

    pub fn new(mantissa: i64, exponent: i8) -> Decimal {
        Decimal{ mantissa, exponent }
    }
    pub fn mantissa(&self) -> i64 {
        self.mantissa
    }
    pub fn exponent(&self) -> i8 {
        self.exponent
    }
    pub fn to_cpon_string(&self) -> String {
        let mut neg = false;
        let (mut mantissa, exponent) = (self.mantissa, self.exponent);
        if mantissa < 0 {
            mantissa = -mantissa;
            neg = true;
        }
        //let buff: Vec<u8> = Vec::new();
        let mut s = mantissa.to_string();

        let n = s.len() as i8;
        let dec_places = -exponent;
        if dec_places > 0 && dec_places < n {
            // insert decimal point
            let dot_ix = n - dec_places;
            s.insert(dot_ix as usize, '.');
        }
        else if dec_places > 0 && dec_places <= 3 {
            // prepend 0.00000..
            let extra_0_cnt = dec_places - n;
            s = "0.".to_string()
                + &*"0".repeat(extra_0_cnt as usize)
                + &*s;
        }
        else if dec_places < 0 && n + exponent <= 9 {
            // append ..000000.
            s += &*"0".repeat(exponent as usize);
            s.push('.');
        }
        else if dec_places == 0 {
            // just append decimal point
            s.push('.');
        }
        else {
            // exponential notation
            s.push('e');
            s += &*exponent.to_string();
        }
        if neg {
            s.insert(0, '-');
        }
        s
    }
    pub fn to_f64(&self) -> f64 {
        let mut d = self.mantissa as f64;
        // We probably don't want to call .cmp() because of performance loss
        #[allow(clippy::comparison_chain)]
        if self.exponent < 0 {
            for _ in self.exponent .. 0 {
                d /= 10.;
            }
        }
        else if self.exponent > 0 {
            for _ in 0 .. self.exponent {
                d *= 10.;
            }
        }
        d
    }
}


