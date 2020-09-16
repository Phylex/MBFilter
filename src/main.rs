use clap::{Arg, App, SubCommand};
use moessbauer_filter::{
    MBConfig,
    MBFilter,
    MBFError,
    MBFState,
};

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
            .arg(Arg::with_name("peak threshhold")
                .short("p")
                .long("pthresh")
                .value_name("peak threshhold")
                .help("minimum value of the peak to be considered as a signal")
                .takes_value(true)
                .required(true)
                .index(4))
            .arg(Arg::with_name("dead time")
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
                .required(true)))
        .subcommand(SubCommand::with_name("stop")
            .about("command that stops the measurement. If the filter is not running the command has no effect"))
        .get_matches();
    if let Some(matches) = matches.subcommand_matches("configure") {
        println!("We are now in the configure section of the program");
    }
    if let Some(matches) = matches.subcommand_matches("start") {
        println!("start subcommand");
    }
    if let Some(matches) = matches.subcommand_matches("stop") {
        println!("stop subcommand");
    }
}
