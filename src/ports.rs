use std::{fmt::Display, ops::Deref, str::FromStr};

#[derive(Clone, Debug)]
pub struct Ports(Vec<u16>);

impl Deref for Ports {
    type Target = Vec<u16>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for Ports {
    type Err = <u16 as FromStr>::Err;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parsed = vec![];
        for part in value.split(',') {
            match part.split_once('-') {
                Some((lower, upper)) => {
                    let (lower, upper) = (lower.parse::<u16>()?, upper.parse::<u16>()?);
                    assert!(
                        lower <= upper,
                        "Expected port range lower limit be lower than upper limit!"
                    );

                    parsed.reserve((upper - lower + 1).into());
                    parsed.extend(lower..=upper);
                }
                None => {
                    let port = part.parse()?;
                    parsed.push(port);
                }
            }
        }

        Ok(Self(parsed))
    }
}

#[derive(Debug)]
pub struct PortsStatus {
    open: Vec<u16>,
    closed: Vec<u16>,
}

impl PortsStatus {
    pub fn new(num_ports: usize) -> Self {
        Self {
            open: Vec::with_capacity(num_ports / 10),
            closed: Vec::with_capacity(num_ports),
        }
    }

    pub fn record(&mut self, port: u16, open: bool) {
        if open {
            self.open.push(port);
        } else {
            self.closed.push(port);
        }
    }

    pub fn sort(&mut self) {
        self.open.sort();
        self.closed.sort();
    }

    fn fmt_vec(vec: &Vec<u16>, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut start = 0;
        for (prev, (idx, now)) in vec.iter().zip(vec.iter().enumerate().skip(1)) {
            if now - prev > 1 {
                if start == idx - 1 {
                    write!(f, "{prev},")?;
                } else {
                    write!(f, "{}-{prev},", vec[start])?;
                }

                start = idx;
            }
        }

        if start == vec.len() - 1 {
            write!(f, "{}", vec[start])
        } else {
            write!(f, "{}-{}", vec[start], vec[vec.len() - 1])
        }
    }
}

impl Display for PortsStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "open: ")?;
        if !self.open.is_empty() {
            Self::fmt_vec(&self.open, f)?;
        } else {
            write!(f, "none")?;
        }

        write!(f, ";")?;

        write!(f, "closed: ")?;
        if !self.closed.is_empty() {
            Self::fmt_vec(&self.closed, f)
        } else {
            write!(f, "none")
        }
    }
}
