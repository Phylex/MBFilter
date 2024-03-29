use clap::{Arg, App, SubCommand};
use tokio::sync::Mutex;
use std::sync::Arc;
use moessbauer_filter::{
    MBConfig,
    MBFilter,
    MBFState,
};
//use moessbauer_data::{
//    MeasuredPeak,
//};
use std::error::Error;
use std::fs::File;
use std::io::{
    BufWriter,
    Write,
};
use std::path::Path;
use mbfilter::MBError;
use log::{
    debug,
};
use warp::Filter;
use futures_util::{
    SinkExt,
    StreamExt,
};

#[derive(Debug)]
struct MBHTTPError(&'static str);

impl warp::reject::Reject for MBHTTPError {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {

    // initiate logger
    env_logger::init();

    // parse the command line
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
            .arg(Arg::with_name("listen")
                .short("l")
                .long("listen")
                .value_name("listen")
                .help("the IP address and port that the server should listen on")
                .takes_value(true)
                .required(true)
                .index(1)))
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
                .required(true)
                .index(2)))
        .subcommand(SubCommand::with_name("status")
            .about("command that returns the current state of the hardware filter with the currently loaded configuration"))
        .subcommand(SubCommand::with_name("stop")
            .about("stops the filter if it is running"))
        .get_matches();

    // configure subcommand
    if let Some(matches) = matches.subcommand_matches("configure") {
        let filter = MBFilter::new()?;
        let config = MBConfig::new_from_str(
                    matches.value_of("k").unwrap(),
                    matches.value_of("l").unwrap(),
                    matches.value_of("m").unwrap(),
                    matches.value_of("pthresh").unwrap(),
                    matches.value_of("dead-time").unwrap())?;
        filter.configure(config);
        ()
    }

    // start subcommand
    if let Some(matches) = matches.subcommand_matches("start") {
        let mut filter = MBFilter::new()?;
        let requested_pc = u64::from_str_radix(matches.value_of("target file size").unwrap(), 10)?;
        let filepath = matches.value_of("output file").unwrap();
        let path = Path::new(filepath);
        let ofile = File::create(&path)?;
        let mut ofile = BufWriter::new(ofile);
        let mut fc: u64 = 0;
        match filter.state() {
            MBFState::Ready => {
                filter.start();
                let mut buffer: [u8; 12*2048] = [0; 12*2048];
                while fc < requested_pc {
                    let bytes_read = filter.read(&mut buffer)?;
                    debug!("{} bytes read", bytes_read);
                    let mut pos = 0;
                    while pos < (&buffer[..bytes_read]).len() {
                        let bytes_written = ofile.write(&buffer[pos..bytes_read])?;
                        pos += bytes_written;
                    };
                    fc += bytes_read as u64;
                }
                filter.stop();
            },
            _ => Err(MBError::WrongState)?,
        }
    }


    // stop subcommand
    if let Some(_) = matches.subcommand_matches("stop") {
        unimplemented!("stop subcommand")
    }

    // status subcommand
    if let Some(_) = matches.subcommand_matches("status") {
        if let Ok(filter) = MBFilter::new() {
            let config = filter.configuration();
            let state = filter.state();
            println!("{}\nCurrent filter State:\n{}", config, state);
        }
    }

    // server subcommand
    if let Some(matches) = matches.subcommand_matches("server") {
        let filter = Arc::new(Mutex::new(MBFilter::new()?));
        let state_check_filter_copy = filter.clone();
        let socket_address: std::net::SocketAddr = matches.value_of("listen").unwrap().parse()?;
        let route = warp::path("websocket")
            .and(warp::query::query())
            .and_then(validate_config)
            .and_then(move |config| check_and_configure_filter(config, state_check_filter_copy.clone()))
            .and(warp::ws())
            .map(move |config, ws| {
                ws_handler(filter.clone(), config, ws)
            });
        warp::serve(route)
            .run(socket_address)
            .await;
    }
    Ok(())
}

async fn validate_config(config: MBConfig) -> Result<MBConfig, warp::reject::Rejection> {
    match config.validate() {
        Ok(config) => Ok(config),
        Err(_) => Err(warp::reject::custom(MBHTTPError("Invalid Config"))),
    }
}

async fn check_and_configure_filter(config: MBConfig, filter: Arc<Mutex<MBFilter>>) -> Result<MBConfig, warp::reject::Rejection> {
    if let Ok(ref mut unlocked_filter) = filter.try_lock() {
        match unlocked_filter.state() {
            MBFState::Ready | MBFState::InvalidParameters => {
                unlocked_filter.configure(config.clone());
                let read_config = unlocked_filter.configuration();
                if read_config != config {
                    return Err(warp::reject::custom(MBHTTPError("Filter config load error")));
                }
                return Ok(read_config)
            },
            _ => return Err(warp::reject::custom(MBHTTPError("Filter already running"))),
        }
    }
    return Err(warp::reject::custom(MBHTTPError("Filter already running")));
}

// the ws.on_upgrade gives us the reply we want but we still need to handle the rejections that
// can occurr before we reach this function that actually replies with a valid HTTP response
fn ws_handler(filter: Arc<Mutex<MBFilter>>, _config: MBConfig, ws: warp::ws::Ws) -> impl warp::Reply {
    ws.on_upgrade(|websocket| {
        async move {
            {
                let mut locked_filter = filter.lock().await;
                locked_filter.start();
            }
            let (mut wstx, mut wsrx) = websocket.split();
            let reader_filter_clone = filter.clone();
            let control_filter_clone = filter.clone();
            // the task to read a filter
            tokio::spawn(async move {
                let mut buffer: [u8;2048*12] = [0;2048*12];
                let mut count;
                loop {
                    {
                        let mut filter = reader_filter_clone.lock().await;
                        debug!("aquired filter lock for reading");
                        count = filter.read(&mut buffer[..]);
                    }
                    match count {
                        Ok(count) => {
                            debug!("read {} bytes", count);
                            if count % 12 == 0 {
                                match wstx.send(warp::ws::Message::binary(&buffer[..count])).await {
                                    Ok(_) => {},
                                    Err(e) => {
                                        debug!("Error encountered writing to the websocket: {:?}", e);
                                        clean_up(reader_filter_clone.clone()).await;
                                        break;
                                    }
                                }
                            } else {
                                debug!("Did not read a multiple of 12 bytes from filter");
                                clean_up(reader_filter_clone.clone()).await;
                                break;
                            }
                        },
                        Err(e) => {
                            debug!("Error encountered reading filter: {}", e);
                            clean_up(reader_filter_clone.clone()).await;
                            break;
                        },
                    }
                }
            });
            // task that receives the stop command and stops the filter
            tokio::spawn(async move {
                while let Some(result) = wsrx.next().await {
                    match result {
                        Ok(msg) => {
                            debug!("received message from client: {:?}", msg);
                            if msg.is_close() {
                                debug!("message says to close the connection -> stop the filter");
                                let mut locked_filter = control_filter_clone.lock().await;
                                debug!("Stopping filter");
                                locked_filter.stop();
                                break;
                            }
                        },
                        Err(e) => {
                            debug!("Error reading from the websocket: {:?}", e);
                            debug!("Stopping Filter");
                            let mut locked_filter = control_filter_clone.lock().await;
                            locked_filter.stop();
                            break;
                        }
                    };
                }
            });
        }
    })
}

async fn clean_up(filter: Arc<Mutex<MBFilter>>) {
    let mut locked_filter = filter.lock().await;
    debug!("filter lock aquired for cleanup operations");
    match locked_filter.state() {
        MBFState::InvalidParameters => {},
        MBFState::FIFOFull{frame_count: _} => {
            let mut buffer: [u8;2048*12] = [0; 2048*12];
            locked_filter.read(&mut buffer).unwrap();
        },
        MBFState::Ready => {},
        MBFState::Running{frame_count: _} => {
            locked_filter.stop();
            let mut buffer: [u8;2048*12] = [0; 2048*12];
            locked_filter.read(&mut buffer).unwrap();
        },
        MBFState::Halted => {
            let mut buffer: [u8;2048*12] = [0; 2048*12];
            locked_filter.stop();
            let _ = locked_filter.read(&mut buffer).unwrap();
        },
    }
}
