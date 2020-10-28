use clap::{Arg, App, SubCommand};
use tokio::sync::Mutex;
use std::sync::Arc;
use moessbauer_filter::{
    MBConfig,
    MBFilter,
    MBFState,
};
use moessbauer_data::{
    MeasuredPeak,
};
use std::error::Error;
use std::fs::File;
use std::io::{
    BufWriter,
    Write,
};
use std::path::Path;
use mbfilter::MBError;
use log::{
    info,
    debug,
    error,
};
use warp::Filter;
use futures_util::SinkExt;
use hex;

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
            .and_then(move |config| check_filter_state(config, state_check_filter_copy.clone()))
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

async fn check_filter_state(config: MBConfig, filter: Arc<Mutex<MBFilter>>) -> Result<MBConfig, warp::reject::Rejection> {
    if let Ok(ref mut unlocked_filter) = filter.try_lock() {
        match unlocked_filter.state() {
            MBFState::Ready | MBFState::InvalidParameters => {
                unlocked_filter.configure(config.clone());
                debug!("State of the filter after configuration: {}", unlocked_filter.state());
                let read_config = unlocked_filter.configuration();
                if read_config != config {
                    panic!("Filter configs don't match: {}\n{}", config, read_config);
                }
                return Ok(config)
            },
            _ => return Err(warp::reject::custom(MBHTTPError("Filter already running"))),
        }
    }
    return Err(warp::reject::custom(MBHTTPError("Filter already running")));
}

async fn validate_config(config: MBConfig) -> Result<MBConfig, warp::reject::Rejection> {
    match config.validate() {
        Ok(config) => Ok(config),
        Err(_) => Err(warp::reject::custom(MBHTTPError("Invalid Config"))),
    }
}

async fn read_task(filter: Arc<Mutex<MBFilter>>, ws: Arc<Mutex<warp::ws::WebSocket>>) -> Result<(),()> {
    let mut ws = ws.lock().await;
    let mut filter = filter.lock().await;
    let mut buffer: [u8;2048*12] = [0; 2048*12];
    match filter.read(&mut buffer[..]) {
        Err(e) => {
            debug!("Error from filter encountered: {}", e);
            Err(())
        },
        Ok(count) => {
            debug!("{} bytes in buffer", count);
            if count%12 == 0 {
                for i in 1..count/12 {
                    ws.send(warp::ws::Message::text(format!("{}\n",hex::encode(&buffer[(i-1)*12..i*12])))).await.map_err(|_| ())?;
                }
            } else {
                panic!("Strange amount of bytes in the read buffer");
            }
            Ok(())
        },
    }
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
            let count = locked_filter.read(&mut buffer).unwrap();
            if count != 2048*12 {
                panic!("filter in weird state, only read {} bytes, should have read {} bytes", count, 2048*12);
            }
        },
    }
}

fn ws_handler(filter: Arc<Mutex<MBFilter>>, config: MBConfig, ws: warp::ws::Ws) -> impl warp::Reply {
    ws.on_upgrade(move |websocket| {
        async move {
            {
                let mut locked_filter = filter.lock().await;
                debug!("the configuration from the web request {}", config);
                locked_filter.configure(config);
                debug!("Current filter state: {}", locked_filter.state());
                let filter_config = locked_filter.configuration();
                debug!("Configuration loaded into the filter: {}", filter_config);
                locked_filter.start();
            }
            let websocket = Arc::new(Mutex::new(websocket));
            loop {
                if let Err(_) = read_task(filter.clone(), websocket.clone()).await {
                    debug!("encountered ws Error");
                    clean_up(filter.clone()).await;
                    break;
                }
            }
        }
    })
}
