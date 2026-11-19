use clap::ValueEnum;
use semfind_core::Distro;

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum DistroFilter {
    Debian,
    Arch,
    Generic,
}

impl From<DistroFilter> for Distro {
    fn from(value: DistroFilter) -> Self {
        match value {
            DistroFilter::Debian => Self::Debian,
            DistroFilter::Arch => Self::Arch,
            DistroFilter::Generic => Self::Generic,
        }
    }
}
