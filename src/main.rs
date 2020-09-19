use clap::{Arg, ArgMatches, App, SubCommand};
use moessbauer_filter::{
    MBConfig,
    MBFilter,
    MBFError,
    MBFState,
};
use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::process::exit;

fn main() {
    let matches = App::new("Moessbauer Filter")
        .version("0.1")
        .author("Alexander Becker <nabla.becker@mailbox.org>")
        .about("Program to interface with the Hardware on the FPGA")
        .subcommand(SubCommand::with_name("configure")
            .about("write a configuration to the filter. If the filter is currently running, the filter is halted,\
                the fifo emptied and then the filter is configured and placed in the ready state")
            .arg(Arg::with_name("k")
                .short("k")
                .long("k-param")
                .value_name("flank steepnes")
                .help("length of the rising and falling flank of the trapezoidal filter in filter clock cycles (8ns)")
                .takes_value(true)
                .required(true)
                .index(1))
            .arg(Arg::with_name("l")
                .short("l")
                .long("l-param")
                .value_name("plateau length")
                .help("length of the plateau of the trapezoidal filters in filter clock cycles")
                .takes_value(true)
                .required(true)
                .index(2))
            .arg(Arg::with_name("m")
                .short("m")
                .long("m-factor")
                .value_name("decay time factor")
                .help("multiplication factor of the filter. Sets the decay time that the filter is sensitive to")
                .takes_value(true)
                .required(true)
                .index(3))
            .arg(Arg::with_name("pthresh")
                .short("p")
                .long("pthresh")
                .value_name("peak threshhold")
                .help("minimum value of the peak to be considered as a signal")
                .takes_value(true)
                .required(true)
                .index(4))
            .arg(Arg::with_name("dead-time")
                .short("d")
                .long("dtime")
                .value_name("dead time")
                .help("the time in which the filter coalesses multiple peaks into a single peak for noise reduction")
                .takes_value(true)
                .required(true)
                .index(5)))
        .subcommand(SubCommand::with_name("server")
            .about("Turn the control program into a server that opens a specified port and waits for client connections")
            .arg(Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("port")
                .help("The port that the server should listen on")
                .takes_value(true)))
        .subcommand(SubCommand::with_name("start")
            .about("command that starts the measurement. The filter has to be configured to be able to start")
            .arg(Arg::with_name("output file")
                .short("o")
                .long("ofile")
                .value_name("output file")
                .help("file path where the results of the measurement are written to CAUTION: Be aware of disk space")
                .takes_value(true)
                .index(1)
                .required(true))
            .arg(Arg::with_name("target file size")
                .short("s")
                .long("target-file-size")
                .help("The file size that should be collected before the measurement is automatically stopped")
                .takes_value(true)
                .required(true)))
        .subcommand(SubCommand::with_name("status")
            .about("command that returns the current state of the hardware filter with the currently loaded configuration"))
        .get_matches();

    // configure subcommand
    if let Some(matches) = matches.subcommand_matches("configure") {
        if let Ok(filter) = MBFilter::new() {
            let config = MBConfig::new_from_str(matches.value_of("k").unwrap(),
                matches.value_of("l").unwrap(),
                matches.value_of("m").unwrap(),
                matches.value_of("pthresh").unwrap(),
                matches.value_of("dead-time").unwrap());
            match config {
                Ok(config) => filter.configure(config),
                Err(MBFError::NoParameters) => exit(1),
                Err(MBFError::InvalidParameterRange) => {
                    println!("The given parameters are out of range\nThe valid ranges are:\n\tk [0,128]\n\tl [0, 128]\n\tm [0,2048]");
                    exit(1);
                },
                Err(something) => {
                    panic!("unknown error: {:?}", something);
                }
            }
        } else {
            println!("The memmory map failed, there may be another instance of this program running");
        }
    }

    // start subcommand
    if let Some(matches) = matches.subcommand_matches("start") {
        if let Ok(mut filter) = MBFilter::new() {
            let requested_pc = match u64::from_str_radix(matches.value_of("peakcount").unwrap(), 10) {
                Ok(val) => val,
                Err(_) => {
                    println!("the value for the peakcount has to be a natural number");
                    exit(1)
                },
            };
            if let Some(filepath) = matches.value_of("output file") {
                let path = Path::new(filepath);
                let mut ofile = match File::create(&path) {
                    Err(why) => panic!("could not create {}: {}", path.display(), why),
                    Ok(file) => file,
                };
                let mut fc: u64 = 0;
                match filter.state() {
                    MBFState::Ready => {
                        filter.start();
                        let mut buffer: [u8; 12*2048] = [0; 12*2048];
                        while fc < requested_pc {
                            let bytes_read = match filter.read(&mut buffer) {
                                Ok(val) => val,
                                Err(e) => {
                                    panic!("Error encountered: {}",e);
                                },
                            };
                            let mut pos = 0;
                            while pos < (&buffer[..bytes_read]).len() {
                                let bytes_written = ofile.write(&buffer[pos..]).unwrap();
                                pos += bytes_written;
                            }
                            match ofile.write(&buffer[..bytes_read]) {
                                Ok(val) => {
                                    if val != bytes_read {
                                        panic!("Could not write all the to the file");
                                    }
                                },
                                Err(e) => {
                                    panic!("The Error {} occured while writing to the file", e);
                                }
                            };
                            fc += bytes_read as u64;
                        }
                    },
                    _ => {
                        println!("The filter is in wrong state please check the state via the state subcommand");
                        exit(1);
                    }
                };
            }
        } else {
            println!("The memmory map failed, there may be another instance of this program running");
        }
    }

    // stop subcommand
    if let Some(_) = matches.subcommand_matches("stop") {
        println!("stop subcommand");
    }

    // status subcommand
    if let Some(_) = matches.subcommand_matches("status") {
        if let Ok(filter) = MBFilter::new() {
            let config = filter.configuration();
            let state = filter.state();
            println!("{}\nCurrent filter State:\n{}", config, state);
        }
    }
}
