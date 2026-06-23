mod app;
mod config;
mod molecule;
mod widget;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("ChemBuilder — MOL2 Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder",
        options,
        Box::new(|_cc| Ok(Box::new(app::Mol2App::default()))),
    )
}
