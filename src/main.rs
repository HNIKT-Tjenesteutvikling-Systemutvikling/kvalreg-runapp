use chrono::Local;
use clap::App;
use clap::Arg;
use colored::*;
use serde::Deserialize;
use serde_json::from_str;
use std::env;
use std::fs;
use std::fs::remove_dir_all;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
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

fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

fn env_var(key: &str) -> io::Result<String> {
    env::var(key).map_err(|e| error(format!("Environment variable {}: {}", key, e)))
}

fn home_dir() -> io::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| error("Home directory not found"))
}

fn mysql_dir() -> io::Result<String> {
    Ok(format!("{}/mysql", env_var("PWD")?))
}

fn run_status(command: &mut Command, description: &str) -> io::Result<ExitStatus> {
    command
        .status()
        .map_err(|e| error(format!("Failed to execute {}: {}", description, e)))
}

fn run_output(command: &mut Command, description: &str) -> io::Result<Output> {
    command
        .output()
        .map_err(|e| error(format!("Failed to execute {}: {}", description, e)))
}

fn run_expecting_success(
    command: &mut Command,
    description: &str,
    on_failure: &str,
) -> io::Result<()> {
    let status = run_status(command, description)?;
    if !status.success() {
        return Err(error(format!("{} (exit status: {})", on_failure, status)));
    }
    Ok(())
}

fn remove_if_exists(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
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

fn start_services() -> io::Result<()> {
    println!("{}", "Starting authorization server...".bright_blue());
    let auth_status = Command::new("auth-server-run").status();
    match auth_status {
        Ok(s) if s.success() => println!("{}", "Authorization server started.".green()),
        Ok(_) => println!(
            "{}",
            "Authorization server may already be running.".yellow()
        ),
        Err(e) => {
            eprintln!("{}", format!("Failed to start auth server: {}", e).red());
            return Err(error(format!("Failed to start auth server: {}", e)));
        }
    }

    // Give the auth server a moment to bind its port before PDP connects
    thread::sleep(Duration::from_secs(2));

    println!("{}", "Starting PDP container...".bright_blue());
    let pdp_status = Command::new("pdp-docker-run").status();
    match pdp_status {
        Ok(s) if s.success() => println!("{}", "PDP container started.".green()),
        Ok(_) => println!("{}", "PDP container may already be running.".yellow()),
        Err(e) => {
            eprintln!("{}", format!("Failed to start PDP: {}", e).red());
            return Err(error(format!("Failed to start PDP: {}", e)));
        }
    }

    println!("{}", "\nServices started:".bright_green());
    println!("  auth-server  — running in background (logs: auth-server-logs)");
    println!("  pdp          — running in Docker      (logs: pdp-docker-logs)");

    Ok(())
}

fn stop_services() -> io::Result<()> {
    println!("{}", "Stopping PDP container...".yellow());
    let pdp_status = Command::new("pdp-docker-stop").status();
    match pdp_status {
        Ok(s) if s.success() => println!("{}", "PDP container stopped.".green()),
        Ok(_) => println!("{}", "PDP container was not running.".yellow()),
        Err(e) => eprintln!("{}", format!("Failed to stop PDP: {}", e).red()),
    }

    println!("{}", "Stopping authorization server...".yellow());
    let auth_status = Command::new("auth-server-stop").status();
    match auth_status {
        Ok(s) if s.success() => println!("{}", "Authorization server stopped.".green()),
        Ok(_) => println!("{}", "Authorization server was not running.".yellow()),
        Err(e) => eprintln!("{}", format!("Failed to stop auth server: {}", e).red()),
    }

    println!("{}", "\nAll services stopped.".bright_green());

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────

fn clean_up(register_name: &str) -> io::Result<()> {
    println!("{}", "Cleaning up and stopping services...".yellow());
    let status = run_status(
        Command::new("sh").arg("-c").arg("stop_tomcat 2>/dev/null"),
        "stop_tomcat",
    )?;
    if !status.success() {
        println!("{}", "Tomcat not running. Continuing...".yellow());
    }

    run_status(
        Command::new("docker-compose").arg("down"),
        "docker-compose down",
    )?;

    println!("{}", "Cleaning up and stopping MySQL...".yellow());
    if fs::metadata("mysql/data").is_ok() {
        if fs::metadata("mysql/socket.lock").is_ok() {
            run_status(
                Command::new("sh")
                    .arg("-c")
                    .arg("stop_mysql >/dev/null 2>&1"),
                "stop_mysql",
            )?;
            let output = run_output(Command::new("pgrep").arg("mysqld"), "pgrep mysqld")?;
            if !output.stdout.is_empty() {
                run_status(Command::new("pkill").arg("mysqld"), "pkill mysqld")?;
            }
        }

        println!("{}", "\nAwaiting MySQL shutdown...\n".red());
        println!("{}", "Cleaning up files...".yellow());
        thread::sleep(Duration::from_secs(5));

        let my_cnf_path = home_dir()?.join(".my.cnf");
        let mysql_path = mysql_dir()?;
        let catalina_home = env_var("CATALINA_HOME")?;

        remove_if_exists(&my_cnf_path)?;
        remove_if_exists(format!("{}/.my.cnf", mysql_path))?;
        remove_if_exists(format!("{}/.my.cnf", env_var("HOME")?))?;
        remove_if_exists(format!("target/{}.war", register_name))?;
        remove_if_exists(format!("target/{}", register_name))?;
        remove_if_exists("target/war")?;
        remove_if_exists("target/classes")?;
        remove_if_exists("target/generated-sources")?;
        remove_if_exists("target/maven-archiver")?;
        remove_if_exists("target/maven-status")?;
        remove_if_exists(format!("{}/webapps/{}.war", catalina_home, register_name))?;
        remove_if_exists(format!("{}/webapps/{}", catalina_home, register_name))?;
        remove_if_exists(format!("{}/bin/src", catalina_home))?;
        remove_if_exists(format!("{}/logs", catalina_home))?;
        remove_if_exists(format!("{}/compile_log.txt", catalina_home))?;
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
        run_status(&mut Command::new("mysql_drop"), "mysql_drop")?;

        std::thread::sleep(std::time::Duration::from_secs(1));
        remove_if_exists(format!("mysql/{}.sql", register_name))?;
    }

    let my_cnf_path = home_dir()?.join(".my.cnf");
    let catalina_home = env_var("CATALINA_HOME")?;
    let mysql_path = mysql_dir()?;

    remove_if_exists(format!("{}/data", mysql_path))?;
    remove_if_exists(&my_cnf_path)?;
    remove_if_exists(format!("{}/bin/src/*", catalina_home))?;
    remove_if_exists(format!("{}/logs/*", catalina_home))?;
    remove_if_exists(format!("{}/webapps/*", catalina_home))?;
    remove_if_exists("logs/*")?;

    println!("{}", "\nDatabase dropped.".red());

    Ok(())
}

fn clean_local_credentials() -> io::Result<()> {
    let my_cnf_path = home_dir()?.join(".my.cnf");
    let mysql_my_cnf_path = Path::new("mysql/.my.cnf");
    let mvn_compile_log_path = Path::new("tomcat/compile_log.txt");
    let catalina_logs_path = Path::new(&env_var("CATALINA_HOME")?).join("logs");

    println!("{}", my_cnf_path.display());
    println!("{}", "Cleaning up mysql credentials...".yellow());
    remove_if_exists(&my_cnf_path)?;
    remove_if_exists(mysql_my_cnf_path)?;
    remove_if_exists(mvn_compile_log_path)?;
    if !catalina_logs_path.exists() {
        fs::create_dir_all(catalina_logs_path)?;
    }

    Ok(())
}

fn set_mysql_envs<'a>(
    command: &'a mut Command,
    register_name: &str,
) -> io::Result<&'a mut Command> {
    let mysql_unix_port = format!("{}/socket", mysql_dir()?);

    Ok(command
        .env("MYSQL_USER", register_name)
        .env("MYSQL_PASSWORD", register_name)
        .env("MYSQL_UNIX_PORT", &mysql_unix_port)
        .env("MYSQL_DATABASE", register_name))
}

fn setup_local_database(register_name: &str) -> io::Result<()> {
    println!("{}", "\nDatabase setup...".bright_blue());
    println!("{}", "Setting up mysql in env...".yellow());

    let mysql_dir = mysql_dir()?;
    if fs::metadata(&mysql_dir).is_err() {
        fs::create_dir_all(&mysql_dir)?;
    }

    if fs::metadata(format!("{}/data", mysql_dir)).is_err() {
        println!("{}", "No database found. Creating...".red());
        let mut command = Command::new("mysqlinit");
        run_expecting_success(
            set_mysql_envs(&mut command, register_name)?,
            "mysqlinit",
            "Failed to initialize MySQL",
        )?;
    } else {
        println!(
            "{}",
            "\nLocal database already setup. Continuing...".yellow()
        );
    }

    println!("{}", "setting up mysqlcred...".yellow());
    let mut command = Command::new("mysqlcred");
    run_expecting_success(
        set_mysql_envs(&mut command, register_name)?,
        "mysqlcred",
        "Failed to setup MySQL credentials",
    )
}

fn setup_external_database(register_name: &str) -> io::Result<()> {
    println!("{}", "\nsetting up mysqlcred...".yellow());

    run_expecting_success(
        &mut Command::new("mysqlcred"),
        "mysqlcred",
        "Failed to setup MySQL credentials",
    )?;

    if fs::metadata(format!("mysql/{}.sql", register_name)).is_err() {
        println!("{}", "No database found. Creating...".red());
        println!("{}", "Setting up root...".yellow());

        run_expecting_success(
            &mut Command::new("mysqlinit_remote"),
            "mysqlinit_remote",
            "Failed to create MySQL database",
        )?;

        println!("Creating database {}...", register_name);
        fs::File::create(format!("mysql/{}.sql", register_name))?;
    } else {
        println!("{}", "Local database already setup. Continuing...".yellow());

        run_expecting_success(
            &mut Command::new("mysql_infile"),
            "mysql_infile",
            "Failed to load local MySQL file",
        )?;
    }

    Ok(())
}

fn start_database(register_name: &str) -> io::Result<()> {
    let mysql_unix_port = format!("{}/socket", mysql_dir()?);
    let socket_lock_exists = fs::metadata("mysql/socket.lock").is_ok();
    let mysql_running = !run_output(Command::new("pgrep").arg("mysqld"), "pgrep mysqld")?
        .stdout
        .is_empty();

    match (socket_lock_exists, mysql_running) {
        (false, false) => {
            println!(
                "{}",
                "Starting MySQL as no socket.lock file and MySQL is not running...".bright_blue()
            );
            let mut command = Command::new("start_mysql");
            run_status(set_mysql_envs(&mut command, register_name)?, "start_mysql")?;
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
            run_status(Command::new("pkill").arg("mysqld"), "pkill mysqld")?;
            thread::sleep(Duration::from_secs(3));
            let mut command = Command::new("start_mysql");
            run_status(set_mysql_envs(&mut command, register_name)?, "start_mysql")?;
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

fn compile_maven() -> io::Result<()> {
    let target_exists = fs::metadata("target").is_ok();

    if target_exists {
        println!("{}", "Target directory found. Cleaning up...".yellow());
        fs::remove_dir_all("target")?;
    } else {
        println!("{}", "No target directory found...".red());
    }

    let mvn_command = if target_exists { "package" } else { "install" };

    let file = File::create("tomcat/compile_log.txt")?;

    let status = run_status(
        Command::new("mvn")
            .args(["clean", mvn_command, "-DskipTests"])
            .stdout(Stdio::from(file)),
        "mvn",
    )?;

    if !status.success() {
        let file = File::open("tomcat/compile_log.txt")?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
        let last_50_lines = lines
            .iter()
            .rev()
            .take(50)
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        return Err(error(format!(
            "Maven compile failed. Last 50 lines of compile log:\n{}",
            last_50_lines.join("\n")
        )));
    }

    Ok(())
}

fn check_port_8080() -> io::Result<()> {
    let output = run_output(
        Command::new("sh")
            .arg("-c")
            .arg("lsof -i :8080 | grep LISTEN"),
        "lsof -i :8080",
    )?;

    if !output.stdout.is_empty() {
        println!("{}", "Port 8080 is in use by the following process:".red());
        println!("{}", String::from_utf8_lossy(&output.stdout));
        return Err(error("Cannot start Tomcat because port 8080 is in use."));
    }

    Ok(())
}

fn start_tomcat(register_name: &str) -> io::Result<()> {
    println!("{}", "Local environment detected...".bright_blue());
    println!("{}", "Setting up Tomcat...".yellow());

    let catalina_home = env_var("CATALINA_HOME")?;
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
    run_status(
        Command::new("sh")
            .arg("-c")
            .arg(format!("{}/bin/catalina.sh jpda start", catalina_home)),
        "catalina.sh jpda start",
    )?;

    Ok(())
}

fn copy_dir_to(src_dir: &Path, dst_dir: &Path) -> io::Result<()> {
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

fn copy_db_files() -> io::Result<()> {
    let catalina_home = env_var("CATALINA_HOME")?;
    let db_path = format!("{}/bin/src/main/resources/db/application", catalina_home);
    let db_path = Path::new(&db_path);

    if !db_path.exists() {
        println!("{}", "DB path does not exist. Creating...".yellow());
        fs::create_dir_all(db_path)?;
    }

    println!("{}", "Copying db files...".yellow());
    let src_path = Path::new("./src/main/resources/db/application/");
    copy_dir_to(src_path, db_path)?;

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

    println!(
        "{}",
        "\n--------------------------------------------------".bright_green()
    );
    println!(
        "{}",
        format!("   Application finished at: {}", end_timestamp).bright_green()
    );
    println!(
        "{}",
        format!("   Total execution time: {}", format_duration(duration)).bright_green()
    );
    println!(
        "{}",
        "--------------------------------------------------".bright_green()
    );
}

fn join_compile(handle: thread::JoinHandle<io::Result<()>>) -> io::Result<()> {
    handle
        .join()
        .map_err(|_| error("Maven compile thread panicked"))?
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", format!("Error: {}", e).red());
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let start_time = Instant::now();

    let services_flag = Arg::new("services")
        .short('s')
        .long("services")
        .takes_value(false)
        .help("Also start auth-server and PDP after the main setup");

    let matches = App::new("runapp")
        .version("1.0")
        .author("Gako358 <gako358@outlook.com>")
        .about("Sets up environment for running the application")
        .arg(services_flag.clone())
        .subcommand(
            App::new("local")
                .about("Sets up local environment")
                .arg(services_flag.clone()),
        )
        .subcommand(
            App::new("code")
                .about("Sets up environment for VScode")
                .arg(services_flag.clone()),
        )
        .subcommand(
            App::new("docker")
                .about("Sets up environment for Docker")
                .arg(services_flag.clone()),
        )
        .subcommand(
            App::new("test")
                .about("Sets up environment for testing")
                .arg(services_flag.clone()),
        )
        .subcommand(App::new("clean").about("Cleans up and stops services"))
        .subcommand(App::new("drop").about("Cleans up, stops services and drops database"))
        .subcommand(App::new("services-start").about("Start auth-server and PDP container"))
        .subcommand(App::new("services-stop").about("Stop auth-server and PDP container"))
        .get_matches();

    let output = run_output(
        Command::new("nix-instantiate")
            .arg("--eval")
            .arg("--json")
            .arg("register.nix"),
        "nix-instantiate register.nix",
    )?;

    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| error(format!("register.nix output is not valid UTF-8: {}", e)))?;
    let register: Register = from_str(output_str)
        .map_err(|e| error(format!("Failed to parse register.nix output: {}", e)))?;
    let register_name = Arc::new(register.register_name);

    if let Some(matches) = matches.subcommand_matches("local") {
        println!("{}", "Checking if port 8080 is in use...".yellow());
        check_port_8080()?;
        println!("{}", "Stopping running services...".red());
        run_status(
            Command::new("sh").arg("-c").arg("stop_tomcat 2>/dev/null"),
            "stop_tomcat",
        )?;
        let handle = thread::spawn(compile_maven);
        clean_local_credentials()?;
        setup_local_database(&register_name)?;
        start_database(&register_name)?;
        join_compile(handle)?;
        start_tomcat(&register_name)?;
        if matches.is_present("services") {
            start_services()?;
        }
    } else if let Some(matches) = matches.subcommand_matches("code") {
        println!("{}", "Checking if port 8080 is in use...".yellow());
        check_port_8080()?;
        println!("{}", "Stopping running services...".red());
        run_status(
            Command::new("sh").arg("-c").arg("stop_tomcat 2>/dev/null"),
            "stop_tomcat",
        )?;
        let handle = thread::spawn(compile_maven);
        clean_local_credentials()?;
        setup_external_database(&register_name)?;
        join_compile(handle)?;
        start_tomcat(&register_name)?;
        if matches.is_present("services") {
            start_services()?;
        }
    } else if let Some(matches) = matches.subcommand_matches("docker") {
        println!("{}", "Stopping running services...".red());
        run_status(
            Command::new("docker-compose").arg("down"),
            "docker-compose down",
        )?;
        let handle = thread::spawn(compile_maven);
        clean_local_credentials()?;
        setup_local_database(&register_name)?;
        start_database(&register_name)?;
        join_compile(handle)?;
        run_status(
            Command::new("docker")
                .arg("build")
                .arg("-t")
                .arg(format!("{}:latest", *register_name)),
            "docker build",
        )?;
        if matches.is_present("services") {
            start_services()?;
        }
    } else if let Some(matches) = matches.subcommand_matches("test") {
        let handle = thread::spawn(compile_maven);
        join_compile(handle)?;
        copy_db_files()?;
        start_tomcat(&register_name)?;
        if matches.is_present("services") {
            start_services()?;
        }
    } else if let Some(_matches) = matches.subcommand_matches("clean") {
        stop_services()?;
        clean_up(&register_name)?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("drop") {
        stop_services()?;
        clean_up(&register_name)?;
        drop_database(&register_name)?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("services-start") {
        start_services()?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else if let Some(_matches) = matches.subcommand_matches("services-stop") {
        stop_services()?;
        exit_timestamp(start_time);
        std::process::exit(0);
    } else {
        let handle = thread::spawn(compile_maven);
        clean_local_credentials()?;
        setup_external_database(&register_name)?;
        join_compile(handle)?;
        copy_db_files()?;
        if matches.is_present("services") {
            start_services()?;
        }
    }

    exit_timestamp(start_time);

    Ok(())
}
