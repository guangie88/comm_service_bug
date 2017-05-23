#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate log;
extern crate log4rs;
extern crate structopt;

#[macro_use]
extern crate structopt_derive;

use std::io::{self, Write};
use std::process::{self, Command, Output};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

mod errors {
    error_chain! {
        errors {}
    }
}

use errors::*;

#[derive(StructOpt, Debug)]
#[structopt(name = "Comm Service Bug Finder", about = "Program to find the cmd bug in Windows 7.")]
struct MainConfig {
    #[structopt(short = "c", long = "command", help = "Command to run", default_value = "echo hello")]
    cmd: String,

    #[structopt(short = "i", long = "interval", help = "Trigger interval in milliseconds", default_value = "100")]
    interval: u32,

    #[structopt(short = "l", long = "log-config-path", help = "Log config file path")]
    log_config_path: String,
}

fn run_cmd(cmd: &str) -> std::result::Result<Output, io::Error> {
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", &cmd])
            .output()
    } else {
        Command::new("sh")
            .args(&["-c", &cmd])
            .output()
    }
}

fn run() -> Result<()> {
    let config = MainConfig::from_args();

    log4rs::init_file(&config.log_config_path, Default::default())
       .chain_err(|| format!("Unable to initialize log4rs logger with the given config file at '{}'", config.log_config_path))?;

    info!("Config: {:?}", config);

    let childs = (0..)
        .map(|index| {
            thread::sleep(Duration::from_millis(config.interval as u64));
            (index, run_cmd(&config.cmd))
        });

    for (index, child) in childs {
        match child {
            Ok(output) => info!("#{}: {:?}", index, output),
            Err(err) => error!("#{}: {:?}", index, err),
        }
    }

    Ok(())
}

fn main() {
    match run() {
        Ok(_) => {
            println!("Program completed!");
            process::exit(0)
        },

        Err(ref e) => {
            let stderr = &mut io::stderr();

            writeln!(stderr, "Error: {}", e)
                .expect("Unable to write error into stderr!");

            for e in e.iter().skip(1) {
                writeln!(stderr, "- Caused by: {}", e)
                    .expect("Unable to write error causes into stderr!");
            }

            process::exit(1);
        },
    }
}