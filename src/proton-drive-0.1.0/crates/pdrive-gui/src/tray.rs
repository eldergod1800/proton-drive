use ksni::menu::StandardItem;
use ksni::{MenuItem, Tray};

pub struct PdriveTray {
    pub on_open: Box<dyn Fn() + Send>,
    pub on_pause: Box<dyn Fn() + Send>,
    pub on_resume: Box<dyn Fn() + Send>,
    pub on_quit: Box<dyn Fn() + Send>,
}

impl Tray for PdriveTray {
    fn id(&self) -> String {
        "pdrive".to_string()
    }

    fn title(&self) -> String {
        "Proton Drive".to_string()
    }

    fn icon_name(&self) -> String {
        "network-server".to_string()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            MenuItem::Standard(StandardItem {
                label: "Open Proton Drive".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_open)()),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Pause Sync".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_pause)()),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "Resume Sync".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_resume)()),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| (tray.on_quit)()),
                ..Default::default()
            }),
        ]
    }
}
