use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::path::Path;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::str;
use clap::App;
use colored::*;
use std::io::{self, ErrorKind};

#[derive(Deserialize)]
struct Register {
    #[serde(rename = "registerName")]
    register_name: String,
}

fn remove_if_exists(path: &str) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn clean_up(register_name: &str) -> io::Result<()> {
    println!("{}", "Cleaning up and stopping services...".bright_blue());
    Command::new("stop_tomcat")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    Command::new("docker-compose")
        .arg("down")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    println!("{}", "Cleaning up and stopping MySQL...".yellow());
    if fs::metadata("mysql/data").is_ok() {
        if fs::metadata("mysql/socket.lock").is_ok() {
            Command::new("stop_mysql")
                .status()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

            let output = Command::new("pgrep")
                .arg("mysqld")
                .output()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
            if !output.stdout.is_empty() {
                Command::new("pkill")
                    .arg("mysqld")
                    .status()
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
            }
        }

        println!("{}", "\nAwaiting MySQL shutdown...\n".red());
        thread::sleep(Duration::from_secs(5));

        remove_if_exists("mysql/.my.cnf")?;
        remove_if_exists(&format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
        remove_if_exists(&format!("target/{}.war", register_name))?;
        remove_if_exists(&format!("target/{}", register_name))?;
        remove_if_exists(&format!("{}/webapps/{}.war", env::var("CATALINA_HOME").unwrap(), register_name))?;
        remove_if_exists(&format!("{}/webapps/{}", env::var("CATALINA_HOME").unwrap(), register_name))?;
        remove_if_exists(&format!("{}/bin/src/*", env::var("CATALINA_HOME").unwrap()))?;
        remove_if_exists(&format!("{}/logs/*", env::var("CATALINA_HOME").unwrap()))?;
        remove_if_exists("jdk/*")?;
        remove_if_exists("logs/*")?;

        println!("{}", "Stopped running processes".red());
    } else {
        println!("{}", "No local database found. Continuing...".yellow());
    }

    Ok(())
}

fn drop_database(register_name: &str) -> io::Result<()> {
    println!("{}", "Starting to drop database...".bright_blue());

    let mysql_drop_db = env::var("MYSQL_DROP_DB").expect("MYSQL_DROP_DB must be set");
    if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
        println!("Dropping external database {}...", register_name);
        Command::new("mysql")
            .arg("<")
            .arg(&mysql_drop_db)
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        std::thread::sleep(std::time::Duration::from_secs(1));
        remove_if_exists(&format!("mysql/{}.sql", register_name))?;
    }

    remove_if_exists("mysql/data")?;

    let home_dir = env::var("HOME").unwrap();
    remove_if_exists("mysql/.my.cnf")?;
    remove_if_exists(&format!("{}/.my.cnf", home_dir))?;

    let catalina_home = env::var("CATALINA_HOME").unwrap();
    remove_if_exists(&format!("{}/bin/src/*", catalina_home))?;
    remove_if_exists(&format!("{}/logs/*", catalina_home))?;
    remove_if_exists(&format!("{}/webapps/*", catalina_home))?;
    remove_if_exists("logs/*")?;

    println!("{}", "\nDatabase dropped.".red());

    Ok(())
}

fn clean_local_credentials() -> io::Result<()> {
    println!("{}", "Cleaning up local credentials...".bright_blue());

    let home_my_cnf = format!("{}/.my.cnf", env::var("HOME").unwrap());
    remove_if_exists("mysql/.my.cnf")?;
    remove_if_exists(&home_my_cnf)?;

    Ok(())
}

fn setup_local_database() -> io::Result<()> {
    println!("{}", "\nDatabase setup...".bright_blue());
    println!("{}", "Setting up mysql in env...".yellow());

    if fs::metadata("mysql/data").is_err() {
        println!("{}", "No database found. Creating...".red());
        Command::new("mysqlinit")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
    } else {
        println!("{}", "\nLocal database already setup. Continuing...".yellow());
    }

    println!("{}", "setting up mysqlcred...".yellow());
    Command::new("mysqlcred")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    Ok(())
}

fn setup_external_database(register_name: &str) -> io::Result<()> {
    println!("{}", "\nsetting up mysqlcred...".bright_blue());

    Command::new("mysqlcred")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_err() {
        println!("{}", "No database found. Creating...".red());
        println!("{}", "Setting up root...".yellow());

        let mysql_create_db = env::var("MYSQL_CREATE_DB").map_err(|_| io::Error::new(io::ErrorKind::Other, "MYSQL_CREATE_DB must be set"))?;
        Command::new("mysql")
            .arg("<")
            .arg(mysql_create_db)
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        println!("Creating database {}...", register_name);
        fs::File::create(format!("mysql/{}.sql", register_name))?;
    } else {
        println!("{}", "Local database already setup. Continuing...".yellow());

        let mysql_local_load_file = env::var("MYSQL_LOCAL_LOAD_FILE").map_err(|_| io::Error::new(io::ErrorKind::Other, "MYSQL_LOCAL_LOAD_FILE must be set"))?;
        Command::new("mysql")
            .arg("<")
            .arg(mysql_local_load_file)
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
    }

    Ok(())
}

fn start_database() -> io::Result<()> {
    let mysql_local_load_file = env::var("MYSQL_LOCAL_LOAD_FILE").map_err(|_| io::Error::new(io::ErrorKind::Other, "MYSQL_LOCAL_LOAD_FILE must be set"))?;
    let socket_lock_exists = fs::metadata("mysql/socket.lock").is_ok();
    let mysql_running = !Command::new("pgrep").arg("mysqld").output()?.stdout.is_empty();

    if !socket_lock_exists && !mysql_running {
        println!("{}", "Starting MySQL as no socket.lock file and MySQL is not running...".bright_blue());
        Command::new("start_mysql")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        set_local_inline_files_permissions(&mysql_local_load_file)?;

        thread::sleep(Duration::from_secs(3));
    } else if socket_lock_exists && mysql_running {
        println!("{}", "socket.lock file exists and MySQL is running. Continuing...".yellow());
        set_local_inline_files_permissions(&mysql_local_load_file)?;
    } else if !socket_lock_exists && mysql_running {
        println!("{}", "MySQL is running, but no socket.lock file found. Killing MySQL and restarting...".red());
        Command::new("pkill")
            .arg("mysqld")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        thread::sleep(Duration::from_secs(3));
        Command::new("start_mysql")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

        set_local_inline_files_permissions(&mysql_local_load_file)?;

        thread::sleep(Duration::from_secs(3));
    }

    Ok(())
}

fn set_local_inline_files_permissions(mysql_local_load_file: &str) -> io::Result<()> {
    println!("{}", "Setting load local inline files permissions...".yellow());
    let status = Command::new("mysql")
        .arg("-S")
        .arg("MYSQL_UNIX_PORT")
        .arg("<")
        .arg(mysql_local_load_file)
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Command failed"))
    }
}

fn compile_maven() -> std::io::Result<()> {
    if fs::metadata("target").is_err() {
        println!("{}", "No target directory found...".red());
        Command::new("mvn")
            .arg("clean")
            .arg("install")
            .arg("-DskipTests")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
    } else {
        println!("{}", "Target directory found. Cleaning up...".yellow());
        fs::remove_dir_all("target")?;
        Command::new("mvn")
            .arg("clean")
            .arg("package")
            .arg("-DskipTests")
            .status()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;
    }

    Ok(())
}

fn start_tomcat(register_name: &str) -> std::io::Result<()> {
    println!("{}", "Local environment detected...".bright_blue());
    println!("{}", "Setting up Tomcat...".yellow());

    let catalina_home = env::var("CATALINA_HOME").map_err(|_| io::Error::new(io::ErrorKind::Other, "CATALINA_HOME must be set"))?;
    let war_path = format!("{}/webapps/{}.war", catalina_home, register_name);
    let webapp_path = format!("{}/webapps/{}", catalina_home, register_name);

    if fs::metadata(&war_path).is_ok() {
        println!("{}", "delete old war file...".red());
        fs::remove_file(&war_path)?;
    }
    if fs::metadata(&webapp_path).is_ok() {
        println!("{}", "delete webapps folder...".red());
        fs::remove_dir_all(&webapp_path)?;
    }

    println!("{}", "Deploying new WAR...".yellow());
    fs::copy(format!("target/{}.war", register_name), &war_path)?;

    println!("{}", "Starting Tomcat...".yellow());
    Command::new("sh")
        .arg("-c")
        .arg("start_tomcat")
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    Ok(())
}

fn copy_db_files() -> std::io::Result<()> {
    let catalina_home = env::var("CATALINA_HOME").map_err(|_| io::Error::new(io::ErrorKind::Other, "CATALINA_HOME must be set"))?;
    let db_path = format!("{}/bin/src/main/resources/db/application", catalina_home);

    if !Path::new(&db_path).exists() {
        fs::create_dir_all(&db_path)?;
    }

    println!("{}", "Copying db files...".yellow());
    Command::new("cp")
        .arg("-r")
        .arg("./src/main/resources/db/application/")
        .arg(&db_path)
        .status()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to execute command"))?;

    Ok(())
}

fn main() -> std::io::Result<()> {
    let matches = App::new("runapp")
        .version("1.0")
        .author("Gako358 <gako358@outlook.com>")
        .about("Sets up environment for running the application")
        .subcommand(App::new("local")
            .about("Sets up local environment"))
        .subcommand(App::new("code")
            .about("Sets up environment for VScode"))
        .subcommand(App::new("docker")
            .about("Sets up environment for Docker"))
        .subcommand(App::new("clean")
            .about("Cleans up and stops services"))
        .subcommand(App::new("drop")
            .about("Cleans up, stops services and drops database"))
        .get_matches();

    let output = Command::new("nix-instantiate")
        .arg("--eval")
        .arg("--json")
        .arg("register.nix")
        .output()
        .expect("Failed to execute command");

    let output_str = str::from_utf8(&output.stdout).expect("Failed to convert output to string");
    let register: Register = from_str(output_str).expect("Failed to parse JSON");
    let register_name = register.register_name;

    if let Some(_matches) = matches.subcommand_matches("local") {
        let compile_handle = std::thread::spawn(|| {
            compile_maven()
        });

        println!("{}", "Stopping running services...".yellow());
        Command::new("stop_tomcat").status().expect("Failed to execute command");
        clean_local_credentials()?;
        setup_local_database()?;
        start_database()?;

        match compile_handle.join().expect("Thread panicked") {
            Ok(_) => println!("Maven compiled successfully"),
            Err(e) => eprintln!("Failed to compile Maven: {}", e),
        }

        start_tomcat(&register_name)?;
        println!("{}", "Finished setting up environment for local...".green());
    } else if let Some(_matches) = matches.subcommand_matches("code") {
        let compile_handle = std::thread::spawn(|| {
            compile_maven()
        });

        println!("{}", "Stopping running services...".red());
        Command::new("stop_tomcat").status().expect("Failed to execute command");
        clean_local_credentials()?;
        setup_external_database(&register_name)?;

        match compile_handle.join().expect("Thread panicked") {
            Ok(_) => println!("Maven compiled successfully"),
            Err(e) => eprintln!("Failed to compile Maven: {}", e),
        }

        start_tomcat(&register_name)?;
        println!("{}", "Finished setting up environment for VScode...".green());
    } else if let Some(_matches) = matches.subcommand_matches("docker") {
        let compile_handle = std::thread::spawn(|| {
            compile_maven()
        });

        println!("{}", "Stopping running services...".red());
        Command::new("docker-compose").arg("down").status().expect("Failed to execute command");
        clean_local_credentials()?;
        setup_local_database()?;
        start_database()?;

        match compile_handle.join().expect("Thread panicked") {
            Ok(_) => println!("Maven compiled successfully"),
            Err(e) => eprintln!("Failed to compile Maven: {}", e),
        }

        Command::new("docker").arg("build").arg("-t").arg(format!("{}:latest", register_name)).status().expect("Failed to execute command");
        println!("{}", "Starting services...".blue());
        Command::new("docker-compose").arg("up").arg("-d").status().expect("Failed to execute command");
        println!("{}", "Finished setting up environment for Docker...".green());
    } else if let Some(_matches) = matches.subcommand_matches("clean") {
        clean_up(&register_name)?;
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("drop") {
        clean_up(&register_name)?;
        drop_database(&register_name)?;
        std::process::exit(0);
    } else {
        let compile_handle = std::thread::spawn(|| {
            compile_maven()
        });

        clean_local_credentials()?;
        setup_external_database(&register_name)?;

        match compile_handle.join().expect("Thread panicked") {
            Ok(_) => println!("Maven compiled successfully"),
            Err(e) => eprintln!("Failed to compile Maven: {}", e),
        }

        copy_db_files()?;
        println!("{}", "Finished setting up environment for Intellij...".red());
        println!("{}", "Start the tomcat server from inside Intellij...".blue());
    }

    Ok(())
}