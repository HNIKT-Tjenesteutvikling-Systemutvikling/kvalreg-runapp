use clap::App;
use chrono::Local;
use colored::*;
use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::fs;
use std::fs::remove_dir_all;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

extern crate dirs;

#[derive(Deserialize)]
struct Register {
    #[serde(rename = "registerName")]
    register_name: String,
}

fn remove_if_exists(path: &str) -> io::Result<()> {
    let path = Path::new(path);
    if path.exists() {
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    remove_dir_all(entry.path())?;
                } else if entry.path().is_file() {
                    fs::remove_file(entry.path())?;
                }
            }
            fs::remove_dir(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn clean_up(register_name: &str) -> io::Result<()> {
    println!("{}", "Cleaning up and stopping services...".yellow());
    let result = Command::new("sh")
        .arg("-c")
        .arg("stop_tomcat 2>/dev/null")
        .status();

    match result {
        Ok(status) => {
            if !status.success() {
                println!("{}", "Tomcat not running. Continuing...".yellow());
            }
        }
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to execute command",
            ));
        }
    }

    Command::new("docker-compose")
        .arg("down")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    println!("{}", "Cleaning up and stopping MySQL...".yellow());
    if fs::metadata("mysql/data").is_ok() {
        if fs::metadata("mysql/socket.lock").is_ok() {
            Command::new("sh")
                .arg("-c")
                .arg("stop_mysql >/dev/null 2>&1")
                .status()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
            let output = Command::new("pgrep")
                .arg("mysqld")
                .output()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
            if !output.stdout.is_empty() {
                Command::new("pkill").arg("mysqld").status().map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "Failed to execute command")
                })?;
            }
        }

        println!("{}", "\nAwaiting MySQL shutdown...\n".red());
        println!("{}", "Cleaning up files...".yellow());
        thread::sleep(Duration::from_secs(5));

        let home_dir = dirs::home_dir().expect("Home directory not found");
        let my_cnf_path = home_dir.join(".my.cnf");
        let mysql_path = format!("{}/mysql", std::env::var("PWD").unwrap());

        remove_if_exists(my_cnf_path.to_str().unwrap())?;
        remove_if_exists(&format!("{}/.my.cnf", mysql_path))?;
        remove_if_exists(&format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
        remove_if_exists(&format!("target/{}.war", register_name))?;
        remove_if_exists(&format!("target/{}", register_name))?;
        remove_if_exists(&format!("target/war"))?;
        remove_if_exists(&format!("target/classes"))?;
        remove_if_exists(&format!("target/generated-sources"))?;
        remove_if_exists(&format!("target/maven-archiver"))?;
        remove_if_exists(&format!("target/maven-status"))?;
        remove_if_exists(&format!(
            "{}/webapps/{}.war",
            env::var("CATALINA_HOME").unwrap(),
            register_name
        ))?;
        remove_if_exists(&format!(
            "{}/webapps/{}",
            env::var("CATALINA_HOME").unwrap(),
            register_name
        ))?;
        remove_if_exists(&format!("{}/bin/src", env::var("CATALINA_HOME").unwrap()))?;
        remove_if_exists(&format!("{}/logs", env::var("CATALINA_HOME").unwrap()))?;
        remove_if_exists(&format!(
            "{}/compile_log.txt",
            env::var("CATALINA_HOME").unwrap()
        ))?;
        remove_if_exists("jdk/*")?;
        remove_if_exists("logs/*")?;
        remove_if_exists("overlays/*")?;

        println!("{}", "Stopped running processes".red());
    } else {
        println!("{}", "No local database found. Continuing...".yellow());
    }

    Ok(())
}

fn drop_database(register_name: &str) -> io::Result<()> {
    println!("{}", "Starting to drop database...".bright_blue());

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
        println!("Dropping external database {}...", register_name);
        Command::new("mysql_drop")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        std::thread::sleep(std::time::Duration::from_secs(1));
        remove_if_exists(&format!("mysql/{}.sql", register_name))?;
    }

    let home_dir = dirs::home_dir().expect("Home directory not found");
    let my_cnf_path = home_dir.join(".my.cnf");
    let catalina_home = env::var("CATALINA_HOME").unwrap();
    let mysql_path = format!("{}/mysql", std::env::var("PWD").unwrap());

    remove_if_exists(&format!("{}/data", mysql_path))?;
    remove_if_exists(my_cnf_path.to_str().unwrap())?;
    remove_if_exists(&format!("{}/bin/src/*", catalina_home))?;
    remove_if_exists(&format!("{}/logs/*", catalina_home))?;
    remove_if_exists(&format!("{}/webapps/*", catalina_home))?;
    remove_if_exists("logs/*")?;

    println!("{}", "\nDatabase dropped.".red());

    Ok(())
}

fn clean_local_credentials() -> std::io::Result<()> {
    let home_dir = dirs::home_dir().expect("Home directory not found");
    let my_cnf_path = home_dir.join(".my.cnf");
    let mysql_my_cnf_path = Path::new("mysql/.my.cnf");
    let mvn_compile_log_path = Path::new("tomcat/compile_log.txt");
    let catalina_logs_path = env::var("CATALINA_HOME")
        .map(|path| Path::new(&path).join("logs"))
        .unwrap();

    println!("{}", my_cnf_path.to_str().unwrap());
    println!("{}", "Cleaning up mysql credentials...".yellow());
    remove_if_exists(my_cnf_path.to_str().unwrap())?;
    remove_if_exists(mysql_my_cnf_path.to_str().unwrap())?;
    remove_if_exists(mvn_compile_log_path.to_str().unwrap())?;
    if !catalina_logs_path.exists() {
        fs::create_dir_all(catalina_logs_path)?;
    }

    Ok(())
}

fn set_mysql_envs<'a>(command: &'a mut Command, register_name: &str) -> &'a mut Command {
    let mysql_dir = format!("{}/mysql", std::env::var("PWD").unwrap());
    let mysql_unix_port = format!("{}/socket", &mysql_dir);

    command
        .env("MYSQL_USER", register_name)
        .env("MYSQL_PASSWORD", register_name)
        .env("MYSQL_UNIX_PORT", &mysql_unix_port)
        .env("MYSQL_DATABASE", register_name)
}

fn setup_local_database(register_name: &str) -> std::io::Result<()> {
    println!("{}", "\nDatabase setup...".bright_blue());
    println!("{}", "Setting up mysql in env...".yellow());

    let mysql_dir = format!("{}/mysql", std::env::var("PWD").unwrap());
    if fs::metadata(&mysql_dir).is_err() {
        fs::create_dir_all(&mysql_dir)?;
    }

    if fs::metadata(format!("{}/data", &mysql_dir)).is_err() {
        println!("{}", "No database found. Creating...".red());
        let mut command = Command::new("mysqlinit");
        let status = set_mysql_envs(&mut command, register_name)
            .status()
            .expect("Failed to execute command");
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to initialize MySQL",
            ));
        }
    } else {
        println!(
            "{}",
            "\nLocal database already setup. Continuing...".yellow()
        );
    }

    println!("{}", "setting up mysqlcred...".yellow());
    let mut command = Command::new("mysqlcred");
    let status = set_mysql_envs(&mut command, register_name)
        .status()
        .expect("Failed to execute command");
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to setup MySQL credentials",
        ));
    }

    Ok(())
}

fn setup_external_database(register_name: &str) -> std::io::Result<()> {
    println!("{}", "\nsetting up mysqlcred...".yellow());

    let status = Command::new("mysqlcred")
        .status()
        .expect("Failed to execute command");
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to setup MySQL credentials",
        ));
    }

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_err() {
        println!("{}", "No database found. Creating...".red());
        println!("{}", "Setting up root...".yellow());

        let status = Command::new("mysqlinit_remote")
            .status()
            .expect("Failed to execute command");
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to create MySQL database",
            ));
        }

        println!("Creating database {}...", register_name);
        fs::File::create(format!("mysql/{}.sql", register_name))?;
    } else {
        println!("{}", "Local database already setup. Continuing...".yellow());

        let status = Command::new("mysql_infile")
            .status()
            .expect("Failed to execute command");
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to load local MySQL file",
            ));
        }
    }

    Ok(())
}

fn start_database(register_name: &str) -> std::io::Result<()> {
    let mysql_dir = format!("{}/mysql", std::env::var("PWD").unwrap());
    let mysql_unix_port = format!("{}/socket", &mysql_dir);
    let socket_lock_exists = fs::metadata("mysql/socket.lock").is_ok();
    let mysql_running = !Command::new("pgrep")
        .arg("mysqld")
        .output()?
        .stdout
        .is_empty();

    match (socket_lock_exists, mysql_running) {
        (false, false) => {
            println!(
                "{}",
                "Starting MySQL as no socket.lock file and MySQL is not running...".bright_blue()
            );
            let mut command = Command::new("start_mysql");
            set_mysql_envs(&mut command, register_name)
                .status()
                .expect("Failed to execute command");
            thread::sleep(Duration::from_secs(3));
        }
        (true, true) => {
            println!(
                "{}",
                "socket.lock file exists and MySQL is running. Continuing...".yellow()
            );
        }
        (false, true) => {
            println!(
                "{}",
                "MySQL is running, but no socket.lock file found. Killing MySQL and restarting..."
                    .red()
            );
            Command::new("pkill")
                .arg("mysqld")
                .status()
                .expect("Failed to execute command");
            thread::sleep(Duration::from_secs(3));
            let mut command = Command::new("start_mysql");
            set_mysql_envs(&mut command, register_name)
                .status()
                .expect("Failed to execute command");
            thread::sleep(Duration::from_secs(3));
        }
        _ => {}
    }

    println!(
        "{}",
        "Setting load local inline files permissions...".yellow()
    );
    thread::sleep(Duration::from_secs(3));
    let mut command = Command::new("mysql_infile");
    command
        .env("MYSQL_UNIX_PORT", &mysql_unix_port)
        .status()
        .expect("Failed to execute command");
    thread::sleep(Duration::from_secs(3));

    Ok(())
}

fn compile_maven() -> Result<(), String> {
    let target_exists = fs::metadata("target").is_ok();

    if target_exists {
        println!("{}", "Target directory found. Cleaning up...".yellow());
        fs::remove_dir_all("target").map_err(|e| e.to_string())?;
    } else {
        println!("{}", "No target directory found...".red());
    }

    let mvn_command = if target_exists { "package" } else { "install" };

    let file = File::create("tomcat/compile_log.txt").map_err(|e| e.to_string())?;

    let status = Command::new("mvn")
        .args(&["clean", mvn_command, "-DskipTests"])
        .stdout(Stdio::from(file))
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        let file = File::open("tomcat/compile_log.txt").map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader
            .lines()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let last_50_lines = lines
            .iter()
            .rev()
            .take(50)
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        return Err(format!(
            "Maven compile failed. Last 50 lines of compile log:\n{}",
            last_50_lines.join("\n")
        ));
    }

    Ok(())
}

fn check_port_8080() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("lsof -i :8080 | grep LISTEN")
        .output()
        .expect("Failed to execute command");

    if !output.stdout.is_empty() {
        println!("{}", "Port 8080 is in use by the following process:".red());
        println!("{}", String::from_utf8_lossy(&output.stdout));
        panic!("Cannot start Tomcat because port 8080 is in use.");
    }
}

fn start_tomcat(register_name: &str) -> std::io::Result<()> {
    println!("{}", "Local environment detected...".bright_blue());
    println!("{}", "Setting up Tomcat...".yellow());

    let catalina_home = env::var("CATALINA_HOME").unwrap();
    let war_file_path = format!("{}/webapps/{}.war", catalina_home, register_name);
    let webapp_folder_path = format!("{}/webapps/{}", catalina_home, register_name);

    if fs::metadata(&war_file_path).is_ok() {
        println!("{}", "delete old war file...".red());
        fs::remove_file(&war_file_path)?;
    }
    if fs::metadata(&webapp_folder_path).is_ok() {
        println!("{}", "delete webapps folder...".red());
        fs::remove_dir_all(&webapp_folder_path)?;
    }

    println!("{}", "Deploying new WAR...".yellow());
    fs::copy(format!("target/{}.war", register_name), &war_file_path)?;

    println!("Starting Tomcat...");
    Command::new("sh")
        .arg("-c")
        .arg(format!("{}/bin/catalina.sh jpda start", catalina_home))
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn copy_dir_to(src_dir: &Path, dst_dir: &Path) -> std::io::Result<()> {
    if !dst_dir.is_dir() {
        fs::create_dir_all(dst_dir)?;
    }

    for entry_result in src_dir.read_dir()? {
        let entry = entry_result?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst_dir.join(entry.file_name());

        if file_type.is_file() {
            fs::copy(src_path, dst_path)?;
        } else if file_type.is_dir() {
            copy_dir_to(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn copy_db_files() -> std::io::Result<()> {
    let catalina_home = env::var("CATALINA_HOME").unwrap();
    let db_path = format!("{}/bin/src/main/resources/db/application", catalina_home);
    let db_path = Path::new(&db_path);

    if !db_path.exists() {
        println!("{}", "DB path does not exist. Creating...".yellow());
        fs::create_dir_all(&db_path)?;
    }

    println!("{}", "Copying db files...".yellow());
    let src_path = Path::new("./src/main/resources/db/application/");
    copy_dir_to(&src_path, &db_path)?;

    Ok(())
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let millis = duration.subsec_millis();
    
    if minutes > 0 {
        format!("{}m {}s {}ms", minutes, seconds, millis)
    } else {
        format!("{}s {}ms", seconds, millis)
    }
}

fn exit_timestamp(start_time: Instant) {
    let end_timestamp = Local::now().format("%d-%m-%Y %H:%M:%S").to_string();
    let duration = Instant::now().duration_since(start_time);
    
    println!("{}", format!("\n--------------------------------------------------").bright_green());
    println!("{}", format!("   Application finished at: {}", end_timestamp).bright_green());
    println!("{}", format!("   Total execution time: {}", format_duration(duration)).bright_green());
    println!("{}", format!("--------------------------------------------------").bright_green());
}

fn main() -> std::io::Result<()> {
    let start_time = Instant::now();
    
    let matches = App::new("runapp")
        .version("1.0")
        .author("Gako358 <gako358@outlook.com>")
        .about("Sets up environment for running the application")
        .subcommand(App::new("local").about("Sets up local environment"))
        .subcommand(App::new("code").about("Sets up environment for VScode"))
        .subcommand(App::new("docker").about("Sets up environment for Docker"))
        .subcommand(App::new("test").about("Sets up environment for testing"))
        .subcommand(App::new("clean").about("Cleans up and stops services"))
        .subcommand(App::new("drop").about("Cleans up, stops services and drops database"))
        .get_matches();

    let output = Command::new("nix-instantiate")
        .arg("--eval")
        .arg("--json")
        .arg("register.nix")
        .output()
        .expect("Failed to execute command");

    let output_str = str::from_utf8(&output.stdout).unwrap();
    let register: Register = from_str(output_str).expect("Failed to parse JSON");
    let register_name = Arc::new(register.register_name);

    if let Some(_matches) = matches.subcommand_matches("local") {
        println!("{}", "Checking if port 8080 is in use...".yellow());
        check_port_8080();
        println!("{}", "Stopping running services...".red());
        Command::new("sh")
            .arg("-c")
            .arg("stop_tomcat 2>/dev/null")
            .status()
            .expect("Failed to execute command");
        let handle = thread::spawn(|| {
            compile_maven().expect("Failed to compile Maven");
        });
        clean_local_credentials()?;
        setup_local_database(&register_name)?;
        start_database(&register_name)?;
        handle.join().unwrap();
        start_tomcat(&*register_name)?;
    } else if let Some(_matches) = matches.subcommand_matches("code") {
        println!("{}", "Checking if port 8080 is in use...".yellow());
        check_port_8080();
        println!("{}", "Stopping running services...".red());
        Command::new("sh")
            .arg("-c")
            .arg("stop_tomcat 2>/dev/null")
            .status()
            .expect("Failed to execute command");
        let handle = thread::spawn(|| {
            compile_maven().expect("Failed to compile Maven");
        });
        clean_local_credentials()?;
        setup_external_database(&*register_name)?;
        handle.join().unwrap();
        start_tomcat(&*register_name)?;
    } else if let Some(_matches) = matches.subcommand_matches("docker") {
        println!("{}", "Stopping running services...".red());
        Command::new("docker-compose")
            .arg("down")
            .status()
            .expect("Failed to execute command");
        let handle = thread::spawn(|| {
            compile_maven().expect("Failed to compile Maven");
        });
        clean_local_credentials()?;
        setup_local_database(&register_name)?;
        start_database(&register_name)?;
        handle.join().unwrap();
        Command::new("docker")
            .arg("build")
            .arg("-t")
            .arg(format!("{}:latest", &*register_name))
            .status()
            .expect("Failed to execute command");
    } else if let Some(_matches) = matches.subcommand_matches("test") {
        let handle = thread::spawn(|| {
            compile_maven().expect("Failed to compile Maven");
        });
        handle.join().unwrap();
        copy_db_files().unwrap();
        start_tomcat(&*register_name)?;
    } else if let Some(_matches) = matches.subcommand_matches("clean") {
        clean_up(&*register_name)?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("drop") {
        clean_up(&*register_name)?;
        drop_database(&*register_name)?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else {
        let handle = thread::spawn(|| {
            compile_maven().expect("Failed to compile Maven");
        });
        clean_local_credentials()?;
        setup_external_database(&*register_name)?;
        handle.join().unwrap();
        copy_db_files().unwrap();
    }

    exit_timestamp(start_time);

    Ok(())
}
