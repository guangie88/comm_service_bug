#[macro_use]
extern crate derive_new;

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate log;
extern crate log4rs;
extern crate reqwest;

#[macro_use]
extern crate rouille;

#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate structopt;

#[macro_use]
extern crate structopt_derive;

use reqwest::Client;
use rouille::{Request, Response};
use std::io::{self, Read, Write};
use std::process::{self, Command, Output};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

mod errors {
    error_chain! {
        errors {}
    }
}

use errors::*;

#[derive(Serialize, Deserialize, Debug, new)]
struct ExecOutput {
    index: u64,
    status: Option<i32>,
    stdout: String,
    stderr: String,
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

fn execute(index: Arc<RwLock<u64>>, config: Arc<MainConfig>) -> Result<ExecOutput> {
    info!("Server received /execute");

    let mut index = match (*index).write() {
        Ok(index) => index,
        Err(_) => bail!("Unable to write into index for increment"),
    };
        
    let cur_index = *index;
    *index = cur_index + 1;

    run_cmd(&(*config).cmd)
        .map(|output| {
            ExecOutput::new(
                cur_index,
                output.status.code().clone(),
                String::from_utf8_lossy(&output.stdout).to_string(),
                String::from_utf8_lossy(&output.stderr).to_string())
        })
        .chain_err(|| "Unable to perform execute")
}

#[derive(StructOpt, Debug)]
#[structopt(name = "Comm Service Bug Finder", about = "Program to find the cmd bug in Windows 7.")]
struct MainConfig {
    #[structopt(short = "c", long = "command", help = "Command to run", default_value = "echo hello")]
    cmd: String,

    #[structopt(short = "i", long = "interval", help = "Trigger interval in milliseconds", default_value = "100")]
    interval: u32,

    #[structopt(short = "p", long = "port", help = "Port to host", default_value = "17385")]
    port: u16,

    #[structopt(short = "l", long = "log-config-path", help = "Log config file path")]
    log_config_path: String,
}

fn route(request: &Request, index: Arc<RwLock<u64>>, config: Arc<MainConfig>) -> Response {
    router!(request,
        (POST) (/execute) => {
            let combine_err = |index, config| -> Result<String> {
                let exec_output = execute(index, config)
                    .chain_err(|| "Error in executing command")?;

                serde_json::to_string(&exec_output)
                    .chain_err(|| "Unable to deserialize executed output into JSON")
            };

            match combine_err(index, config) {
                Ok(exec_content) => {
                    Response::text(exec_content)
                },

                Err(e) => {
                    Response::text(format!("{}", e))
                        .with_status_code(400)
                },
            }
        },

        _ => Response::text("Attempting to get response from invalid endpoint / via invalid method")
            .with_status_code(404)
    )
}

fn run() -> Result<()> {
    let config = MainConfig::from_args();

    log4rs::init_file(&config.log_config_path, Default::default())
       .chain_err(|| format!("Unable to initialize log4rs logger with the given config file at '{}'", config.log_config_path))?;

    info!("Config: {:?}", config);

    // client side
    let port = config.port;
    let sleep_ms = config.interval as u64;

    thread::spawn(move || {
        loop {
            let client_fn = || -> Result<String> {
                let client = Client::new()
                    .chain_err(|| "Error creating HTTP client")?;

                let rsp = client.post(&format!("http://localhost:{}/execute", port))
                    .send();

                match rsp {
                    Ok(mut rsp) => {
                        if rsp.status().is_success() {
                            let mut content = String::new();
                            let _ = rsp.read_to_string(&mut content);

                            Ok(format!("Client succeeded in sending command, body: {}", content))
                        } else {
                            bail!("Client succeeded in sending command, but returned status code: {:?}", rsp.status());
                        }
                    },

                    Err(e) => {
                        bail!("Client failed to send command: {}", e);
                    },
                }
            };

            thread::sleep(Duration::from_millis(sleep_ms));
            
            match client_fn() {
                Ok(msg) => info!("{}", msg),
                Err(e) => error!("HTTP thread error: {}", e),
            }
        }
    });

    // server side

    let index = Arc::new(RwLock::new(0u64));
    let config = Arc::new(config);

    rouille::start_server(&format!("0.0.0.0:{}", config.port), move |request| {
        let index = index.clone();
        let config = config.clone();

        route(&request, index, config)
    });
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