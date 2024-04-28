#![windows_subsystem = "windows"]
use chrono::prelude::*;
use csv::*;
use fltk::prelude::*;
use fltk::{app::*, button::*, dialog::*, enums::FrameType, frame::*, misc::*, text::*, window::*};
use serde::Deserialize;
use std::io::prelude::*;
use std::{fs::OpenOptions, io::Write, sync::Arc, sync::RwLock, thread};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Start,
    Stop,
    File,
    Calibrate,
}

#[derive(Debug, Clone, Copy)]
struct C_Values {
    c1: i32,
    c2: i32,
    c3: i32,
    c4: i32,
}

#[derive(Debug, Deserialize)]
struct OneLine {
    cnt: i32,
    v1: i32,
    v2: i32,
    v3: i32,
    v4: i32,
    v5: i32,
    v6: i32,
    v7: i32,
    v8: i32,
    v9: i32,
    v10: i32,
}

fn main() {
    // Thread Status Variable with R/W Locks
    let running = Arc::new(RwLock::new(0));

    // Get app handle
    let app = App::default();

    // Place to put the filename
    let mut file_name: String = String::new();

    // Place to put calibration values
    let mut c = C_Values {
        c1: 0,
        c2: 0,
        c3: 0,
        c4: 0,
    };

    // Main Window
    let mut wind = Window::new(100, 100, 800, 530, "Electrostatic Data Logger v1.0");

    // Output and Com Port text boxes
    let mut output: SimpleTerminal = SimpleTerminal::new(10, 10, 385, 400, "");
    let mut frame: Frame = Frame::new(405, 10, 385, 400, "");
    let mut com_port: InputChoice = InputChoice::new(350, 420, 80, 30, "COM Port");
    let mut com_settings: InputChoice = InputChoice::new(350, 470, 80, 30, "COM Baud");

    frame.set_frame(FrameType::EmbossedFrame);

    output.set_stay_at_bottom(true);
    output.set_ansi(false);
    output.set_cursor_style(Cursor::Normal);

    let bauds: Vec<&str> = vec!["1200", "9600", "19200", "115200"];

    for b in bauds {
        com_settings.add(b);
    }

    // Look for usable COM ports and populate drop down
    let ports = serialport::available_ports().expect("No ports found!");
    for p in ports {
        com_port.add(&p.port_name);
    }

    // Define Buttons
    let mut start_button = Button::new(30, 420, 100, 40, "Start");
    let mut stop_button = Button::new(30, 470, 100, 40, "Stop");
    let mut file_button = Button::new(150, 470, 100, 40, "File");
    let mut calibrate_button = Button::new(150, 420, 100, 40, "Calibrate");

    // Make sure Stop button is grayed out initially
    stop_button.deactivate();

    // Show the window
    wind.end();
    wind.show();

    // Setup the message handler
    let (s, r) = channel::<Message>();

    // Attach messages to event emitters
    start_button.emit(s, Message::Start);
    stop_button.emit(s, Message::Stop);
    file_button.emit(s, Message::File);
    calibrate_button.emit(s, Message::Calibrate);

    // Main Message Loop
    while app.wait() {
        if let Some(msg) = r.recv() {
            match msg {
                Message::Start => start(
                    &running,
                    &mut com_port,
                    &mut com_settings,
                    &file_name,
                    &mut output,
                    &mut start_button,
                    &mut stop_button,
                    &mut calibrate_button,
                    &mut c,
                ),
                Message::Stop => stop(
                    &running,
                    &mut start_button,
                    &mut stop_button,
                    &mut calibrate_button,
                ),
                Message::File => file_name = file_chooser(&app),
                Message::Calibrate => {
                    c = calibrate(
                        &running,
                        &mut com_port,
                        &mut com_settings,
                        &mut output,
                        &mut start_button,
                        &mut stop_button,
                        &mut calibrate_button,
                    )
                }
            }
        }
    }
}

// Start logging to CSV
fn start(
    running: &Arc<RwLock<i32>>,
    com_port: &mut InputChoice,
    com_settings: &mut InputChoice,
    file_name: &String,
    output: &mut SimpleTerminal,
    start_button: &mut Button,
    stop_button: &mut Button,
    calibrate_button: &mut Button,
    c_values: &mut C_Values,
) {
    // Make sure user has choosen a file
    if file_name == "" {
        return;
    }
    // Toggle the start/stop buttons
    start_button.deactivate();
    stop_button.activate();
    calibrate_button.deactivate();

    // Set thread status to running
    *running.write().unwrap() = 1;

    // Make a clone of the thread status for the sub thread
    let thread_status = Arc::clone(&running);

    // Get a clone the form controls
    let mut out_handle = output.clone();
    let file_name = file_name.clone();
    let mut start_button = start_button.clone();
    let mut stop_button = stop_button.clone();
    let mut calibrate_button = calibrate_button.clone();

    // Get settings for the COM port
    let baud = match com_settings.value() {
        Some(val) => val.parse::<u32>().unwrap(),
        None => return,
    };
    let port = match com_port.value() {
        Some(val) => val,
        None => return,
    };

    // Spawn the subthread to take readings
    thread::spawn(move || {
        // Buffers etc.
        let mut serial_buf: Vec<u8> = vec![0; 1];
        let mut out_buf: Vec<u8> = Vec::new();
        let mut final_buf: Vec<u8> = Vec::new();
        let mut one_line: Vec<u8> = Vec::new();

        // Open the serial port
        let mut serial_port = serialport::new(port, baud).open();
        match serial_port {
            Ok(_) => {}
            Err(_) => {
                out_handle.append("Serial Port Open Error");
                *thread_status.write().unwrap() = 0;
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
                out_handle.append("File Open Error");
                *thread_status.write().unwrap() = 0;
            }
        }

        // Read data and write to window and file
        match f {
            Ok(ref mut f) => {
                match serial_port {
                    Ok(ref mut serial_port) => {
                        // Main Loop to read bytes from the serial port and record them
                        loop {
                            // If the thread status changes to stopped, leave the thread and reset the buttons
                            if *thread_status.read().unwrap() == 0 {
                                start_button.activate();
                                stop_button.deactivate();
                                calibrate_button.activate();
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
                                                // Get timestamp
                                                let mut time_stamp: Vec<u8> = Local::now()
                                                    .format("%Y-%m-%d,%H:%M:%S")
                                                    .to_string()
                                                    .into_bytes();

                                                // Append time stamp and line of data
                                                final_buf.append(&mut time_stamp);
                                                final_buf.append(&mut one_line);
                                                final_buf
                                                    .append(&mut "\n".to_string().into_bytes());

                                                // Send to display window
                                                out_handle.append(
                                                    std::str::from_utf8(&final_buf).unwrap(),
                                                );

                                                // Refresh the terminal window
                                                awake();

                                                // Send to file
                                                match f.write_all(&final_buf) {
                                                    Ok(_) => (),
                                                    Err(_) => {
                                                        *thread_status.write().unwrap() = 0;
                                                    }
                                                };

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
                                Err(_) => {}
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }
    });
}

// Read 30 values to do a calibration to zero the readings
fn calibrate(
    running: &Arc<RwLock<i32>>,
    com_port: &mut InputChoice,
    com_settings: &mut InputChoice,
    output: &mut SimpleTerminal,
    start_button: &mut Button,
    stop_button: &mut Button,
    calibrate_button: &mut Button,
) -> C_Values {
    // Empty struct with 0 for calibration values
    let mut avg = C_Values {
        c1: 0,
        c2: 0,
        c3: 0,
        c4: 0,
    };

    // Toggle the start/stop buttons
    start_button.deactivate();
    calibrate_button.deactivate();
    stop_button.activate();

    // Set thread status to running
    *running.write().unwrap() = 1;

    // Make a clone of the thread status for the sub thread
    let thread_status = Arc::clone(&running);

    // Get a clone the form controls
    let mut out_handle = output.clone();
    let mut start_button = start_button.clone();
    let mut stop_button = stop_button.clone();
    let mut calibrate_button = calibrate_button.clone();

    // Get settings for the COM port
    let baud = match com_settings.value() {
        Some(val) => val.parse::<u32>().unwrap(),
        None => return avg,
    };
    let port = match com_port.value() {
        Some(val) => val,
        None => return avg,
    };

    // Spawn the subthread to take readings
    thread::spawn(move || {
        // Buffers etc.
        let mut serial_buf: Vec<u8> = vec![0; 1];
        let mut out_buf: Vec<u8> = Vec::new();
        let mut final_buf: Vec<u8> = Vec::new();
        let mut one_line: Vec<u8> = Vec::new();
        let mut c = C_Values {
            c1: 0,
            c2: 0,
            c3: 0,
            c4: 0,
        };

        // Place to store our CSV values
        let mut csv_values: OneLine = OneLine {
            cnt: 0,
            v1: 0,
            v2: 0,
            v3: 0,
            v4: 0,
            v5: 0,
            v6: 0,
            v7: 0,
            v8: 0,
            v9: 0,
            v10: 0,
        };

        // Count number of calibration records
        let mut count = 0;

        // Open the serial port
        let mut serial_port = serialport::new(port, baud).open();
        match serial_port {
            Ok(_) => {}
            Err(_) => {
                out_handle.append("Serial Port Open Error");
                *thread_status.write().unwrap() = 0;
            }
        }

        // Read data and write to window and file
        match serial_port {
            Ok(ref mut serial_port) => {
                // Main Loop to read bytes from the serial port and record them
                loop {
                    // If the thread status changes to stopped, leave the thread and reset the buttons
                    if *thread_status.read().unwrap() == 0 {
                        start_button.activate();
                        stop_button.deactivate();
                        calibrate_button.activate();
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
                                        final_buf.append(&mut count.to_string().into_bytes());
                                        final_buf.append(&mut one_line);
                                        final_buf.append(&mut "\n".to_string().into_bytes());

                                        // Break out the CSV into i32 values and store in a struct
                                        let mut reader = ReaderBuilder::new()
                                            .delimiter(b',')
                                            .has_headers(false)
                                            .from_reader(final_buf.as_slice());

                                        for result in reader.deserialize() {
                                            csv_values = result.unwrap();
                                        }

                                        // Send to display window
                                        out_handle.append(&format!(
                                            "{} {} {} {} {}\n",
                                            count,
                                            csv_values.v1,
                                            csv_values.v2,
                                            csv_values.v3,
                                            csv_values.v4
                                        ));

                                        // Keep our totals
                                        c.c1 += csv_values.v1;
                                        c.c2 += csv_values.v2;
                                        c.c3 += csv_values.v3;
                                        c.c4 += csv_values.v4;

                                        // Check to see if we have 30 readings
                                        if count == 30 {
                                            // Find average of the last readings
                                            avg.c1 = c.c1 / count;
                                            avg.c2 = c.c2 / count;
                                            avg.c3 = c.c3 / count;
                                            avg.c4 = c.c4 / count;

                                            // Show averages on the screen
                                            out_handle.append(&format!(
                                                "\nAVG {} {} {} {}\n\n",
                                                avg.c1, avg.c2, avg.c3, avg.c4
                                            ));

                                            awake();
                                            start_button.activate();
                                            stop_button.deactivate();
                                            calibrate_button.activate();

                                            break;
                                        }

                                        // Refresh the terminal window
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
                        Err(_) => {}
                    }
                }
            }
            Err(_) => {}
        }
    });
    
    avg
}

// Stop logging
fn stop(
    running: &Arc<RwLock<i32>>,
    start_button: &mut Button,
    stop_button: &mut Button,
    calibrate_button: &mut Button,
) {
    // Toggle the start/stop buttons
    start_button.activate();
    calibrate_button.activate();
    stop_button.deactivate();

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
