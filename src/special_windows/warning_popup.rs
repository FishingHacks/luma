use iced::{
    Color, Element, Length, Task,
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, svg, text, vertical_space},
    window,
};

use crate::Message;

#[derive(Debug)]
pub struct State {
    pub(crate) message: String,
}

const WARN_ICON: &[u8] = include_bytes!("../../icons/alert-triangle.svg");

impl State {
    pub fn update(&mut self, _: Message) -> Task<Message> {
        Task::none()
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        let row = row![
            svg(svg::Handle::from_memory(WARN_ICON))
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(40.0))
                .style(|_, _| svg::Style {
                    color: Some(Color::from_rgb8(0xfd, 0xc7, 0x00))
                }),
            text(&self.message)
                .width(Length::Fill)
                .size(16)
                .height(Length::Fill)
                .align_y(Vertical::Center),
        ]
        .spacing(10)
        .height(Length::Shrink);
        column![
            row,
            vertical_space().height(Length::Fill),
            container(button("Ok").on_press(Message::Hide(id)))
                .align_x(Horizontal::Center)
                .width(Length::Fill),
        ]
        .padding(20)
        .into()
    }
}
