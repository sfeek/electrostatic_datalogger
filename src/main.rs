#![windows_subsystem = "windows"]
use chrono::prelude::*;
use csv::*;
use fltk::prelude::*;
use fltk::{app::*, button::*, dialog::*, enums::FrameType, frame::*, misc::*, text::*, window::*};
use serde::Deserialize;
use std::io::prelude::*;
use std::{fs::OpenOptions, io::Write, sync::*, thread};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Start,
    Stop,
    File,
}

#[derive(Debug, Clone, Copy)]
struct CValues {
    c1: i32,
    c2: i32,
    c3: i32,
    c4: i32,
}


#[derive(Debug, Deserialize)]
struct OneLineTimeStamp {
    dt: String,
    tm: String,
    v1: i32,
    v2: i32,
    v3: i32,
    v4: i32,
    _v5: i32,
    _v6: i32,
    _v7: i32,
    _v8: i32,
    _v9: i32,
    _v10: i32,
}

fn main() {
    // Thread Status Variable with R/W Locks
    let running = Arc::new(RwLock::new(0));

    // Setup the message channels
    let (s, r) = channel::<Message>();

    // Get app handle
    let app = App::default();

    // Place to put the filename
    let mut file_name: String = String::new();

    // Main Window
    let mut wind = Window::new(100, 100, 800, 530, "Electrostatic Data Logger v1.0");

    // Output and Com Port text boxes
    let mut output: SimpleTerminal = SimpleTerminal::new(10, 10, 385, 400, "");
    let mut frame: Frame = Frame::new(405, 10, 385, 400, "");
    let mut com_port: InputChoice = InputChoice::new(350, 420, 80, 30, "COM Port");

    frame.set_frame(FrameType::EmbossedFrame);

    output.set_stay_at_bottom(true);
    output.set_ansi(false);
    output.set_cursor_style(Cursor::Normal);

    // Look for usable COM ports and populate drop down
    let ports = serialport::available_ports().expect("No ports found!");
    for p in ports {
        com_port.add(&p.port_name);
    }

    // Define Buttons
    let mut start_button = Button::new(30, 420, 100, 40, "Start");
    let mut stop_button = Button::new(30, 470, 100, 40, "Stop");
    let mut file_button = Button::new(150, 470, 100, 40, "File");

    // Attach messages to event emitters
    start_button.emit(s, Message::Start);
    stop_button.emit(s, Message::Stop);
    file_button.emit(s, Message::File);

    // Make sure Stop button is grayed out initially
    stop_button.deactivate();

    // Show the window
    wind.end();
    wind.show();

    // Main Message Loop
    while app.wait() {
        if let Some(msg) = r.recv() {
            match msg {
                Message::Start => {
                    start(
                        &running,
                        &mut com_port,
                        &file_name,
                        &mut output,
                        &mut start_button,
                        &mut stop_button,
                        &mut file_button,
                    );
                }
                Message::Stop => stop(
                    &running,
                    &mut start_button,
                    &mut stop_button,
                    &mut file_button,
                ),
                Message::File => file_name = file_chooser(&app),
            }
        }
    }
}

// Start logging to CSV
fn start(
    running: &Arc<RwLock<i32>>,
    com_port: &mut InputChoice,
    file_name: &String,
    output: &mut SimpleTerminal,
    start_button: &mut Button,
    stop_button: &mut Button,
    file_button: &mut Button,
) {
    let ctime = 30;
    // Set thread status to running
    *running.write().unwrap() = 1;

    // Make sure user has choosen a file
    if file_name == "" {
        output.append(&format!("\nFile Not Chosen Error\n"));
        *running.write().unwrap() = 0;
        return;
    }

    // Toggle the start/stop buttons
    start_button.deactivate();
    stop_button.activate();
    file_button.deactivate();

    // Make a clone of the thread status for the sub thread
    let running = Arc::clone(&running);

    let mut avg = CValues {
        c1: 0,
        c2: 0,
        c3: 0,
        c4: 0,
    };

    // Place to store our calibration readings
    let mut c = CValues {
        c1: 0,
        c2: 0,
        c3: 0,
        c4: 0,
    };

    // Get settings for the COM port
    let baud = 115200;

    let port = match com_port.value() {
        Some(val) => val,
        None => {
            output.append("\nSerial Port Not Chosen Error\n");
            *running.write().unwrap() = 0;
            return;
        }
    };

    // Get a clone the form controls
    let mut out_handle = output.clone();
    let file_name = file_name.clone();
    let mut start_button = start_button.clone();
    let mut stop_button = stop_button.clone();
    let mut file_button = file_button.clone();

    // Spawn the subthread to take readings
    thread::spawn(move || {
        // Buffers etc.
        let mut serial_buf: Vec<u8> = vec![0; 1];
        let mut out_buf: Vec<u8> = Vec::new();
        let mut final_buf: Vec<u8> = Vec::new();
        let mut one_line: Vec<u8> = Vec::new();

        let mut count = 0;

        // Place to store our CSV values
        let mut file_csv_values: OneLineTimeStamp = OneLineTimeStamp {
            dt: "".to_string(),
            tm: "".to_string(),
            v1: 0,
            v2: 0,
            v3: 0,
            v4: 0,
            _v5: 0,
            _v6: 0,
            _v7: 0,
            _v8: 0,
            _v9: 0,
            _v10: 0,
        };

        // Open the serial port
        let mut serial_port = serialport::new(port, baud).open();
        match serial_port {
            Ok(_) => {}
            Err(_) => {
                out_handle.append("\nSerial Port Open Error\n");
                *running.write().unwrap() = 0;
            }
        }

        // Open the file
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&file_name);
        match f {
            Ok(_) => {}
            Err(_) => {
                out_handle.append("\nFile Open Error\n");
                *running.write().unwrap() = 0;
            }
        }

        out_handle.append(&format!("\n*** Calibration Started ***\n"));

        // Read data and write to window and file
        match f {
            Ok(ref mut f) => {
                match serial_port {
                    Ok(ref mut serial_port) => {
                        // Main Loop to read bytes from the serial port and record them
                        loop {
                            // If the thread status changes to stopped, leave the thread and reset the buttons
                            if *running.read().unwrap() == 0 {
                                start_button.activate();
                                stop_button.deactivate();
                                file_button.activate();
                                break;
                            }

                            // Read byte from the port
                            match serial_port.read(serial_buf.as_mut_slice()) {
                                Ok(_) => {
                                    match serial_buf[0] {
                                        // reached end of line, record and display data
                                        13 => {
                                            // Are we on a blank line, if so write out
                                            if out_buf.len() < 3 {
                                                // Add one to the record count
                                                count += 1;

                                                let mut time_stamp: Vec<u8> = Local::now()
                                                    .format("%Y-%m-%d,%H:%M:%S")
                                                    .to_string()
                                                    .into_bytes();

                                                // Append time stamp and line of data
                                                final_buf.append(&mut time_stamp);

                                                //Get the line of CSV into the buffer
                                                final_buf.append(&mut one_line);
                                                final_buf
                                                    .append(&mut "\n".to_string().into_bytes());

                                                // Break out the CSV into i32 values and store in a struct
                                                let mut reader = ReaderBuilder::new()
                                                    .delimiter(b',')
                                                    .has_headers(false)
                                                    .from_reader(final_buf.as_slice());

                                                for result in reader.deserialize() {
                                                    file_csv_values = result.unwrap();
                                                }

                                                // Send to display window
                                                if count < ctime {
                                                    out_handle.append(&format!(
                                                        "{} {} {} {} {}\n",
                                                        count,
                                                        file_csv_values.v1,
                                                        file_csv_values.v2,
                                                        file_csv_values.v3,
                                                        file_csv_values.v4
                                                    ));
                                                }

                                                // Keep our totals
                                                c.c1 += file_csv_values.v1;
                                                c.c2 += file_csv_values.v2;
                                                c.c3 += file_csv_values.v3;
                                                c.c4 += file_csv_values.v4;

                                                // Check to see if we have 30 readings and switch to logging mode
                                                if count == ctime {
                                                    // Find average of the last readings
                                                    avg.c1 = c.c1 / count;
                                                    avg.c2 = c.c2 / count;
                                                    avg.c3 = c.c3 / count;
                                                    avg.c4 = c.c4 / count;

                                                    // Show averages on the screen
                                                    out_handle.append(&format!(
                                                        "\nCalibration {} {} {} {}\n\n",
                                                        avg.c1, avg.c2, avg.c3, avg.c4
                                                    ));

                                                    // Start logging
                                                    out_handle.append(&format!(
                                                        "\n*** Logging Started ***\n"
                                                    ));
                                                }

                                                if count > ctime {
                                                    // Send to display window
                                                    out_handle.append(&format!(
                                                        "{},{},{},{},{},{}\n",
                                                        file_csv_values.dt,
                                                        file_csv_values.tm,
                                                        file_csv_values.v1 - avg.c1,
                                                        file_csv_values.v2 - avg.c2,
                                                        file_csv_values.v3 - avg.c3,
                                                        file_csv_values.v4 - avg.c4,
                                                    ));
                                                    // Make CSV to send to the file
                                                    let file_out: String = format!(
                                                        "{},{},{},{},{},{}\n",
                                                        file_csv_values.dt,
                                                        file_csv_values.tm,
                                                        file_csv_values.v1 - avg.c1,
                                                        file_csv_values.v2 - avg.c2,
                                                        file_csv_values.v3 - avg.c3,
                                                        file_csv_values.v4 - avg.c4,
                                                    );

                                                    // Send to file
                                                    match f.write_all(&file_out.into_bytes()) {
                                                        Ok(_) => (),
                                                        Err(_) => {
                                                            *running.write().unwrap() = 0;
                                                        }
                                                    };
                                                }
                                                awake();

                                                // Clear out buffers for the next line
                                                out_buf.clear();
                                                final_buf.clear();
                                                one_line.clear();
                                            } else {
                                                // Add what we have so far
                                                one_line.append(&mut ",".to_string().into_bytes());
                                                // Keep only the count output
                                                one_line.append(&mut out_buf[4..8].to_vec());
                                                // Clear the output buffer
                                                out_buf.clear();
                                            }
                                        }
                                        // Throw away line feeds
                                        10 => {}
                                        // Keep everything else
                                        _ => out_buf.push(serial_buf[0]),
                                    }
                                }
                                Err(_) => {
                                    out_handle.append(&format!("\nSerial Read Error\n"));
                                }
                            }
                        }
                    }
                    Err(_) => out_handle.append(&format!("\nSerial Port Error\n")),
                }
            }
            Err(_) => out_handle.append(&format!("\nFile Open Error\n")),
        }
    });
}

// Stop logging
fn stop(
    running: &Arc<RwLock<i32>>,
    start_button: &mut Button,
    stop_button: &mut Button,
    file_button: &mut Button,
) {
    // Toggle the start/stop buttons
    start_button.activate();
    stop_button.deactivate();
    file_button.activate();

    // Set thread status to not running
    *running.write().unwrap() = 0;
}

// Handle File Chooser Button
fn file_chooser(app: &App) -> String {
    let mut fc = FileChooser::new(".", "*.csv", FileChooserType::Create, "Choose Output File");

    fc.show();
    fc.window().set_pos(300, 300);

    while fc.shown() {
        app.wait();
    }

    // User hit cancel?
    if fc.value(1).is_none() {
        return String::from("");
    }

    fc.value(1).unwrap()
}
