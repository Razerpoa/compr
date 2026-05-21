pub enum Filter {
    Delta,
    Paeth,
}

impl Filter {
    pub fn apply(&self, data: &[u8], width: usize) -> Vec<u8> {
        match self {
            Filter::Delta => self.apply_delta(data),
            Filter::Paeth => self.apply_paeth(data, width),
        }
    }
    pub fn reverse(&self, data: &[u8], width: usize) -> Vec<u8> {
        match self {
            Filter::Delta => self.reverse_delta(data),
            Filter::Paeth => self.reverse_paeth(data, width),
        }
    }

    fn apply_delta(&self, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let mut prev = 0u8;
        for &byte in data {
            output.push(byte.wrapping_sub(prev));
            prev = byte;
        }
        output
    }

    fn reverse_delta(&self, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let mut prev = 0u8;
        for &byte in data {
            let val = byte.wrapping_add(prev);
            output.push(val);
            prev = val;
        }
        output
    }

    fn apply_paeth(&self, data: &[u8], width: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let height = if width > 0 { data.len() / width } else { 0 };
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                let left = if x > 0 { data[i - 1] } else { 0 };
                let above = if y > 0 { data[i - width] } else { 0 };
                let upper_left = if x > 0 && y > 0 { data[i - width - 1] } else { 0 };
                
                let predictor = paeth_predictor(left, above, upper_left);
                output.push(data[i].wrapping_sub(predictor));
            }
        }
        output
    }

    fn reverse_paeth(&self, data: &[u8], width: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        let height = if width > 0 { data.len() / width } else { 0 };
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                let left = if x > 0 { output[i - 1] } else { 0 };
                let above = if y > 0 { output[i - width] } else { 0 };
                let upper_left = if x > 0 && y > 0 { output[i - width - 1] } else { 0 };
                
                let predictor = paeth_predictor(left, above, upper_left);
                output.push(data[i].wrapping_add(predictor));
            }
        }
        output
    }
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_roundtrip() {
        let data = vec![10, 20, 30, 40, 50];
        let filter = Filter::Delta;
        let applied = filter.apply(&data, 5);
        let reversed = filter.reverse(&applied, 5);
        assert_eq!(data, reversed);
    }

    #[test]
    fn test_paeth_roundtrip() {
        let data = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let filter = Filter::Paeth;
        let applied = filter.apply(&data, 5);
        let reversed = filter.reverse(&applied, 5);
        assert_eq!(data, reversed);
    }
}
