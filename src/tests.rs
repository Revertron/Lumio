#[cfg(test)]
mod tests {
    use gui::traits::View;
    use gui::ui::UI;
    use crate::gui::views::button::Button;
    use gui::themes::Classic;

    #[test]
    fn test() {
        let layout = include_str!("layout.xml");
        let scale = 2.0;
        let mut ui = UI::from_xml(layout, 1920, 1080, Classic::default(), scale).unwrap();
        let mut theme = Classic::new(graphics, 1920, 1080, scale);
        ui.paint(&mut theme);

        if let Some(button) = ui.get_view("btn1") {
            button.borrow_mut().on_event(EventType::Click, Box::new(button1_click));
            if button.borrow_mut().click(&mut ui) {
                println!("Click processed");
            }
        }
        ui.paint(&mut theme);
    }

    fn button1_click(ui: &mut UI, view: &mut dyn View) -> bool {
        let button = ui.create("Button");
        button.borrow_mut().set_any("text", "Button from click!");
        button.borrow_mut().set_any("id", "btn100");
        /*if let Some(b) = button.borrow_mut().downcast_mut::<Button>() {
            b.set_text("Button from click!");
        }*/
        if let Some(frame) = ui.get_view("main") {
            frame.borrow_mut().as_container_mut().unwrap().add_view(button);
        }

        /*if let Some(b) = view.downcast_mut::<Button>() {
            b.set_text("Button clicked!");
        }*/
        true
    }
}
