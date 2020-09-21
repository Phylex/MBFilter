use moessbauer_filter::MBFError;
use std::fmt;
use std::io;
use std::error::Error;

#[derive(Debug)]
pub enum MBError {
    WrongState,
    InvalidInput,
    FilterError(MBFError),
    IOError(io::Error),
}

impl From<MBFError> for MBError {
    fn from(error: MBFError) -> Self {
        MBError::FilterError(error)
    }
}

impl From<io::Error> for MBError {
    fn from(error: io::Error) -> Self {
        MBError::IOError(error)
    }
}

impl fmt::Display for MBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MBError::InvalidInput => write!(f, "The input given on the command line can't be converted into the needed type, please consult the help message"),
            MBError::WrongState => write!(f, "The filter is in the wrong state to perform the desired operation, please use the state subcommand to inspect\
            \nthe state of the filter and bring the filter to the ready state"),
            MBError::FilterError(e) => write!(f, "The Filter-hardware reported an error: {}", e),
            MBError::IOError(e) => write!(f, "An error occured while handeling the File: {}", e),
        }
    }
}

impl Error for MBError {
    fn source (&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MBError::WrongState => None,
            MBError::InvalidInput => None,
            MBError::FilterError(e) => Some(e),
            MBError::IOError(e) => Some(e),
        }
    }
}
