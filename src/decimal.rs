//use crate::rpcvalue::RpcValue;

/// mantisa: 56, exponent: 8;
/// I'm storing whole Decimal in one i64 to keep size_of RpcValue == 24
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Decimal (i64);

impl Decimal {

    pub fn new(mantisa: i64, exponent: i8) -> Decimal {
        //log::debug!("\t mantisa: {} {:b}", mantisa, mantisa);
        let mut n = mantisa << 8;
        //log::debug!("\t 1antisa: {} {:b}", n, n);
        n |= (exponent as i64) & 0xff;
        //log::debug!("\t 2antisa: {} {:b}", n, n);
        Decimal(n)
    }
    pub fn decode(&self) -> (i64, i8) {
        let m = self.0 >> 8;
        let e = self.0 as i8;
        (m, e)
    }
    pub fn mantissa(&self) -> i64 {
        self.decode().0
    }
    pub fn exponent(&self) -> i8 {
        self.decode().1
    }
    pub fn to_cpon_string(&self) -> String {
        let mut neg = false;
        let (mut mantisa, exponent) = self.decode();
        if mantisa < 0 {
            mantisa = -mantisa;
            neg = true;
        }
        //let buff: Vec<u8> = Vec::new();
        let mut s = mantisa.to_string();

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
        let (m, e) = self.decode();
        let mut d = m as f64;
        // We probably don't want to call .cmp() because of performance loss
        #[allow(clippy::comparison_chain)]
        if e < 0 {
            for _ in e .. 0 {
                d /= 10.;
            }
        }
        else if e > 0 {
            for _ in 0 .. e {
                d *= 10.;
            }
        }
        d
    }
}


