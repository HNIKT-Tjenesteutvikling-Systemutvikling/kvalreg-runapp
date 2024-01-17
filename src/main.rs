use clap::App;
use colored::*;
use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::thread;
use std::time::Duration;

#[derive(Deserialize)]
struct Register {
    #[serde(rename = "registerName")]
    register_name: String,
}

fn clean_up(register_name: &str) -> std::io::Result<()> {
    println!("{}", "Cleaning up and stopping services...".bright_blue());
    Command::new("stop_tomcat").output()?;

    Command::new("docker-compose").arg("down").output()?;

    println!("{}", "Cleaning up and stopping MySQL...".yellow());
    if Path::new("mysql/data").exists() {
        if Path::new("mysql/socket.lock").exists() {
            Command::new("stop_mysql").output()?;

            let output = Command::new("pgrep").arg("mysqld").output()?;
            if !output.stdout.is_empty() {
                Command::new("pkill").arg("mysqld").output()?;
            }
        }

        println!("{}", "\nAwaiting MySQL shutdown...".red());
        thread::sleep(Duration::from_secs(5));
        let home_dir =
            env::var("HOME").map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let catalina_home = env::var("CATALINA_HOME")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let paths_to_remove: Vec<PathBuf> = vec![
            Path::new("mysql/.my.cnf").to_path_buf(),
            Path::new(&home_dir).join(".my.cnf"),
            Path::new("target")
                .join(format!("{}.war", register_name))
                .to_path_buf(),
            Path::new("target").join(register_name).to_path_buf(),
            Path::new(&catalina_home)
                .join("webapps")
                .join(format!("{}.war", register_name))
                .to_path_buf(),
            Path::new(&catalina_home)
                .join("webapps")
                .join(register_name)
                .to_path_buf(),
            Path::new(&catalina_home).join("bin/src/*").to_path_buf(),
            Path::new(&catalina_home).join("logs/*").to_path_buf(),
            Path::new("jdk/*").to_path_buf(),
            Path::new("logs/*").to_path_buf(),
        ];
        for path in &paths_to_remove {
            if path.exists() {
                if path.is_dir() {
                    fs::remove_dir_all(path)?;
                } else {
                    fs::remove_file(path)?;
                }
            }
        }

        println!("{}", "Stopped running processes".red());
    } else {
        println!("{}", "No local database found. Continuing...".yellow());
    }

    Ok(())
}

fn drop_database(register_name: &str) -> std::io::Result<()> {
    println!("{}", "Starting to drop database...".bright_blue());

    let mysql_drop_db = env::var("MYSQL_DROP_DB").expect("MYSQL_DROP_DB must be set");
    if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
        println!("Dropping external database {}...", register_name);
        Command::new("mysql")
            .arg("<")
            .arg(&mysql_drop_db)
            .status()
            .expect("Failed to execute command");

        std::thread::sleep(std::time::Duration::from_secs(1));
        if fs::metadata(format!("mysql/{}.sql", register_name)).is_ok() {
            fs::remove_file(format!("mysql/{}.sql", register_name))?;
        }
    }

    if fs::metadata("mysql/data").is_ok() {
        println!("{}", "Dropping local database...".yellow());
        fs::remove_dir_all("mysql/data")?;
    }

    let home_dir = env::var("HOME").unwrap();
    if fs::metadata("mysql/.my.cnf").is_ok() {
        fs::remove_file("mysql/.my.cnf")?;
        if fs::metadata(format!("{}/.my.cnf", home_dir)).is_ok() {
            fs::remove_file(format!("{}/.my.cnf", home_dir))?;
        }
    } else if fs::metadata(format!("{}/.my.cnf", home_dir)).is_ok() {
        fs::remove_file(format!("{}/.my.cnf", home_dir))?;
    }

    let catalina_home = env::var("CATALINA_HOME").unwrap();
    if fs::metadata(format!("{}/bin/src", catalina_home)).is_ok() {
        fs::remove_dir_all(format!("{}/bin/src/*", catalina_home))?;
        fs::remove_dir_all(format!("{}/logs/*", catalina_home))?;
        fs::remove_dir_all(format!("{}/webapps/*", catalina_home))?;
        if fs::metadata("logs/*").is_ok() {
            fs::remove_dir_all("logs/*")?;
        }
    }
    println!("{}", "\nDatabase dropped.".red());

    Ok(())
}

fn clean_local_credentials() -> std::io::Result<()> {
    println!("{}", "Cleaning up local credentials...".bright_blue());

    if fs::metadata("mysql/.my.cnf").is_ok() {
        fs::remove_file("mysql/.my.cnf")?;
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    } else if fs::metadata(format!("{}/.my.cnf", env::var("HOME").unwrap())).is_ok() {
        fs::remove_file(format!("{}/.my.cnf", env::var("HOME").unwrap()))?;
    }

    Ok(())
}

fn setup_local_database() -> std::io::Result<()> {
    println!("{}", "\nDatabase setup...".bright_blue());
    println!("{}", "Setting up mysql in env...".yellow());

    if fs::metadata("mysql/data").is_err() {
        println!("{}", "No database found. Creating...".red());
        Command::new("mysqlinit")
            .status()
            .expect("Failed to execute command");
    } else {
        println!(
            "{}",
            "\nLocal database already setup. Continuing...".yellow()
        );
    }

    println!("{}", "setting up mysqlcred...".yellow());
    Command::new("mysqlcred")
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn setup_external_database(register_name: &str) -> std::io::Result<()> {
    println!("{}", "\nsetting up mysqlcred...".yellow());

    Command::new("mysqlcred")
        .status()
        .expect("Failed to execute command");

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_err() {
        println!("{}", "No database found. Creating...".red());
        println!("{}", "Setting up root...".on_bright_cyan());

        let mysql_create_db = env::var("MYSQL_CREATE_DB").expect("MYSQL_CREATE_DB must be set");
        Command::new("mysql")
            .arg("<")
            .arg(mysql_create_db)
            .status()
            .expect("Failed to execute command");

        println!("Creating database {}...", register_name);
        fs::File::create(format!("mysql/{}.sql", register_name))?;
    } else {
        println!("{}", "Local database already setup. Continuing...".yellow());

        let mysql_local_load_file =
            env::var("MYSQL_LOCAL_LOAD_FILE").expect("MYSQL_LOCAL_LOAD_FILE must be set");
        Command::new("mysql")
            .arg("<")
            .arg(mysql_local_load_file)
            .status()
            .expect("Failed to execute command");
    }

    Ok(())
}

fn start_database() -> std::io::Result<()> {
    let mysql_local_load_file =
        env::var("MYSQL_LOCAL_LOAD_FILE").expect("MYSQL_LOCAL_LOAD_FILE must be set");
    if fs::metadata("mysql/socket.lock").is_err()
        && Command::new("pgrep")
            .arg("mysqld")
            .output()?
            .stdout
            .is_empty()
    {
        println!(
            "{}",
            "Starting MySQL as no socket.lock file and MySQL is not running...".bright_blue()
        );
        Command::new("start_mysql")
            .status()
            .expect("Failed to execute command");

        println!(
            "{}",
            "Setting load local inline files permissions...".yellow()
        );
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");

        thread::sleep(Duration::from_secs(3));
    } else if fs::metadata("mysql/socket.lock").is_ok()
        && !Command::new("pgrep")
            .arg("mysqld")
            .output()?
            .stdout
            .is_empty()
    {
        println!(
            "{}",
            "socket.lock file exists and MySQL is running. Continuing...".yellow()
        );
        println!(
            "{}",
            "Setting load local inline files permissions...".bright_blue()
        );
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");
    } else if fs::metadata("mysql/socket.lock").is_err()
        && !Command::new("pgrep")
            .arg("mysqld")
            .output()?
            .stdout
            .is_empty()
    {
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
        Command::new("start_mysql")
            .status()
            .expect("Failed to execute command");

        println!(
            "{}",
            "Setting load local inline files permissions...".yellow()
        );
        Command::new("mysql")
            .arg("-S")
            .arg("MYSQL_UNIX_PORT")
            .arg("<")
            .arg(&mysql_local_load_file)
            .status()
            .expect("Failed to execute command");

        thread::sleep(Duration::from_secs(3));
    }

    Ok(())
}

fn compile_maven() -> std::io::Result<()> {
    if fs::metadata("target").is_err() {
        println!("{}", "No target directory found...".red());
        Command::new("mvn")
            .arg("clean")
            .arg("install")
            .arg("-DskipTests")
            .status()
            .expect("Failed to execute command");
    } else {
        println!("{}", "Target directory found. Cleaning up...".yellow());
        fs::remove_dir_all("target")?;
        Command::new("mvn")
            .arg("clean")
            .arg("package")
            .arg("-DskipTests")
            .status()
            .expect("Failed to execute command");
    }

    Ok(())
}

fn start_tomcat(register_name: &str) -> std::io::Result<()> {
    println!("{}", "Local environment detected...".bright_blue());
    println!("{}", "Setting up Tomcat...".yellow());

    let catalina_home = env::var("CATALINA_HOME").unwrap();
    if fs::metadata(format!("{}/webapps/{}.war", catalina_home, register_name)).is_ok() {
        println!("{}", "delete old war file...".red());
        fs::remove_file(format!("{}/webapps/{}.war", catalina_home, register_name))?;
    }
    if fs::metadata(format!("{}/webapps/{}", catalina_home, register_name)).is_ok() {
        println!("{}", "delete webapps folder...".red());
        fs::remove_dir_all(format!("{}/webapps/{}", catalina_home, register_name))?;
    }

    println!("{}", "Deploying new WAR...".yellow());
    fs::copy(
        format!("target/{}.war", register_name),
        format!("{}/webapps/{}.war", catalina_home, register_name),
    )?;

    println!("Starting Tomcat...");
    Command::new("sh")
        .arg("-c")
        .arg("start_tomcat")
        .status()
        .expect("Failed to execute command");

    Ok(())
}

fn copy_db_files() -> std::io::Result<()> {
    let catalina_home = env::var("CATALINA_HOME").unwrap();
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
        .expect("Failed to execute command");

    Ok(())
}

fn main() -> std::io::Result<()> {
    let matches = App::new("runapp")
        .version("1.0")
        .author("Gako358 <gako358@outlook.com>")
        .about("Sets up environment for running the application")
        .subcommand(App::new("local").about("Sets up local environment"))
        .subcommand(App::new("code").about("Sets up environment for VScode"))
        .subcommand(App::new("docker").about("Sets up environment for Docker"))
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
    let register_name = register.register_name;

    if let Some(_matches) = matches.subcommand_matches("local") {
        println!("{}", "Stopping running services...".red());
        Command::new("stop_tomcat")
            .status()
            .expect("Failed to execute command");
        clean_local_credentials()?;
        setup_local_database()?;
        start_database()?;
        compile_maven()?;
        start_tomcat(&register_name)?;
        println!("{}", "Finished setting up environment for local...".green());
    } else if let Some(_matches) = matches.subcommand_matches("code") {
        println!("{}", "Stopping running services...".red());
        Command::new("stop_tomcat")
            .status()
            .expect("Failed to execute command");
        clean_local_credentials()?;
        setup_external_database(&register_name)?;
        compile_maven()?;
        start_tomcat(&register_name)?;
        println!(
            "{}",
            "Finished setting up environment for VScode...".green()
        );
    } else if let Some(_matches) = matches.subcommand_matches("docker") {
        println!("{}", "Stopping running services...".red());
        Command::new("docker-compose")
            .arg("down")
            .status()
            .expect("Failed to execute command");
        clean_local_credentials()?;
        setup_local_database()?;
        start_database()?;
        compile_maven()?;
        Command::new("docker")
            .arg("build")
            .arg("-t")
            .arg(format!("{}:latest", register_name))
            .status()
            .expect("Failed to execute command");
        println!("{}", "Starting services...".blue());
        Command::new("docker-compose")
            .arg("up")
            .arg("-d")
            .status()
            .expect("Failed to execute command");
        println!(
            "{}",
            "Finished setting up environment for Docker...".green()
        );
    } else if let Some(_matches) = matches.subcommand_matches("clean") {
        clean_up(&register_name)?;
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("drop") {
        clean_up(&register_name)?;
        drop_database(&register_name)?;
        std::process::exit(0);
    } else {
        clean_local_credentials()?;
        setup_external_database(&register_name)?;
        compile_maven()?;
        copy_db_files()?;
        println!(
            "{}",
            "Finnished setting up environment for Intellij...".red()
        );
        println!(
            "{}",
            "Start the tomcat server from inside Intellij...".blue()
        );
    }

    Ok(())

}