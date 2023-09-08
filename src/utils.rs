#[macro_export]
macro_rules! write_demo {
    ($demo_name:literal, $demo:ident) => {{
        use demosuperimpose_goldsrc::writer::DemoWriter;
        let mut out = DemoWriter::new(String::from($demo_name));
        out.write_file($demo);
    }};
}

#[macro_export]
macro_rules! wrap_message {
    ($svc:ident, $msg:ident) => {{
        let huh = EngineMessage::$svc($msg);
        let hah = Message::EngineMessage(huh);
        hah
    }};
}

#[macro_export]
macro_rules! open_demo {
    ($name:literal) => {{
        let mut bytes = Vec::new();
        let mut f = File::open($name).unwrap();
        f.read_to_end(&mut bytes).unwrap();

        hldemo::Demo::parse(bytes.leak()).unwrap()
    }};
}
