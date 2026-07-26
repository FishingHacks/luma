use iced::{
    Border, Element, Event, Rectangle, Renderer, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        mouse::{Cursor, Interaction},
        widget::{Operation, Tree},
    },
    keyboard::{self, Key, Modifiers, key::Named},
    widget::{Id, TextInput, text_input},
};

use crate::Message;

pub struct SearchInput<'a>(TextInput<'a, Message>);

impl SearchInput<'_> {
    pub fn new(query: &str, id: Id) -> Self {
        let inner = iced::widget::text_input("Search", query)
            .id(id)
            .on_input(Message::UpdateSearch)
            .style(|theme, status| {
                let mut style = text_input::default(theme, status);
                style.border = Border::default().width(0.0);
                style
            });
        Self(inner)
    }
}

impl Widget<Message, Theme, Renderer> for SearchInput<'_> {
    fn size(&self) -> iced::Size<iced::Length> {
        Widget::size(&self.0)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        Widget::layout(&mut self.0, tree, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        Widget::draw(
            &self.0, tree, renderer, theme, style, layout, cursor, viewport,
        );
    }

    fn size_hint(&self) -> iced::Size<iced::Length> {
        self.0.size_hint()
    }

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        self.0.tag()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        self.0.state()
    }

    fn children(&self) -> Vec<Tree> {
        self.0.children()
    }

    fn operate(
        &mut self,
        state: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0.operate(state, layout, renderer, operation);
    }

    fn update(
        &mut self,
        state: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let handled = 'blk: {
            match event {
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key: Key::Named(Named::Alt),
                    ..
                }) => {
                    shell.publish(Message::HideActions);
                }

                Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => {
                    // these are reserved for custom shortcuts.
                    if modifiers.intersects(crate::ALLOWED_ACTION_MODIFIERS) {
                        return;
                    }
                }
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    match key {
                        Key::Named(Named::ArrowLeft | Named::ArrowRight | Named::Backspace)
                            if modifiers.command() =>
                        {
                            break 'blk false;
                        }
                        Key::Character(c)
                            if modifiers.command()
                                && (c == "a" || c == "c" || c == "x" || c == "v") =>
                        {
                            break 'blk false;
                        }
                        // only no modifiers or alt+enter count as submit (alt because of the
                        // actions list that shows up when holding down alt.)
                        Key::Named(Named::Enter) if modifiers.alt() || modifiers.is_empty() => {
                            shell.publish(Message::Submit);
                        }
                        Key::Named(Named::PageUp) => shell.publish(Message::Go10Up),
                        Key::Named(Named::PageDown) => shell.publish(Message::Go10Down),
                        Key::Named(Named::ArrowUp) => shell.publish(Message::GoUp),
                        Key::Named(Named::ArrowDown) => shell.publish(Message::GoDown),
                        Key::Named(Named::Escape) => shell.publish(Message::HideMainWindow),
                        Key::Named(Named::Alt) => shell.publish(Message::ShowActions),
                        // these are reserved for custom shortcuts. All text input shortcuts are
                        // checked above (e.g. C-a, C-backspace)
                        _ if modifiers.intersects(crate::ALLOWED_ACTION_MODIFIERS) => {
                            return;
                        }
                        _ => break 'blk false,
                    }
                }

                _ => break 'blk false,
            }
            true
        };
        if handled {
            shell.capture_event();
            return;
        }
        self.0.update(
            state, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _: &Tree,
        _: Layout<'_>,
        _: Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> Interaction {
        Interaction::Idle
    }
}

impl<'a> From<SearchInput<'a>> for Element<'a, Message> {
    fn from(val: SearchInput<'a>) -> Self {
        Element::new(val)
    }
}
