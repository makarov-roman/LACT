use super::CONTENT_MAXIMUM_WIDTH;
use crate::I18N;
use adw::prelude::*;
use gtk::glib::{self, clone};
use i18n_embed_fl::fl;
use relm4::{ComponentParts, ComponentSender, RelmObjectExt, RelmWidgetExt, binding::BoolBinding};

pub struct PageNavigation {
    pub stack: gtk::Stack,
    pages: Vec<Page>,
}

pub struct PageNavigationInit {
    pub pages: Vec<(&'static str, String, gtk::Widget)>,
    pub parent: adw::ApplicationWindow,
    pub sensitive: BoolBinding,
}

#[derive(Debug)]
pub enum PageNavigationMsg {
    Select(usize),
    Detach(usize),
    Attach(usize),
}

struct Page {
    name: &'static str,
    host: gtk::Box,
    content: gtk::ScrolledWindow,
    placeholder: gtk::Box,
    window: adw::Window,
    toolbar: adw::ToolbarView,
}

#[relm4::component(pub)]
impl relm4::SimpleComponent for PageNavigation {
    type Init = PageNavigationInit;
    type Input = PageNavigationMsg;
    type Output = ();

    view! {
        gtk::ListBox {
            add_css_class: "navigation-sidebar",
            set_margin_vertical: 1,
            set_vexpand: true,
            connect_row_selected[sender] => move |_, row| {
                if let Some(row) = row {
                    sender.input(PageNavigationMsg::Select(row.index() as usize));
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let stack = gtk::Stack::builder().vhomogeneous(false).build();
        let mut pages = Vec::new();

        for (index, (name, title, widget)) in init.pages.into_iter().enumerate() {
            let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let clamp = adw::Clamp::builder()
                .maximum_size(CONTENT_MAXIMUM_WIDTH)
                .tightening_threshold(CONTENT_MAXIMUM_WIDTH)
                .child(&widget)
                .build();
            let content = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .child(&clamp)
                .build();
            content.add_binding(&init.sensitive, "sensitive");
            host.append(&content);

            let toolbar = adw::ToolbarView::new();
            toolbar.add_top_bar(&adw::HeaderBar::new());
            let window = adw::Window::builder()
                .title(&title)
                .default_width(900)
                .default_height(750)
                .transient_for(&init.parent)
                .destroy_with_parent(true)
                .content(&toolbar)
                .build();
            window.connect_close_request(clone!(
                #[strong]
                sender,
                move |_| {
                    sender.input(PageNavigationMsg::Attach(index));
                    glib::Propagation::Stop
                }
            ));

            relm4::view! {
                #[name = "placeholder"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_all: 24,
                    set_align: gtk::Align::Center,
                    set_vexpand: true,
                    set_visible: false,

                    gtk::Label {
                        set_label: &fl!(I18N, "page-detached"),
                    },
                    gtk::Button {
                        set_label: &fl!(I18N, "show-page-window"),
                        connect_clicked[sender] => move |_| {
                            sender.input(PageNavigationMsg::Detach(index));
                        },
                    },
                    gtk::Button {
                        set_label: &fl!(I18N, "reattach-page"),
                        connect_clicked[sender] => move |_| {
                            sender.input(PageNavigationMsg::Attach(index));
                        },
                    },
                },
                #[name = "row"]
                gtk::Box {
                    set_spacing: 6,
                    gtk::Label {
                        set_label: &title,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                    },
                    gtk::Button {
                        set_icon_name: "window-new-symbolic",
                        set_tooltip_text: Some(&fl!(I18N, "detach-page", page = title.as_str())),
                        add_css_class: "flat",
                        connect_clicked[sender] => move |_| {
                            sender.input(PageNavigationMsg::Detach(index));
                        },
                    },
                },
            }
            host.append(&placeholder);
            stack.add_titled(&host, Some(name), &title);
            root.append(&row);
            pages.push(Page {
                name,
                host,
                content,
                placeholder,
                window,
                toolbar,
            });
        }

        let names: Vec<_> = pages.iter().map(|page| page.name).collect();
        stack.connect_visible_child_name_notify(clone!(
            #[weak]
            root,
            move |stack| {
                let index = names
                    .iter()
                    .position(|name| Some(*name) == stack.visible_child_name().as_deref());
                root.select_row(
                    index
                        .and_then(|index| root.row_at_index(index as i32))
                        .as_ref(),
                );
            }
        ));

        let model = Self { stack, pages };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            PageNavigationMsg::Select(index) => {
                self.stack.set_visible_child_name(self.pages[index].name);
            }
            PageNavigationMsg::Detach(index) => self.pages[index].detach(),
            PageNavigationMsg::Attach(index) => self.pages[index].attach(),
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        for page in &self.pages {
            page.window.destroy();
        }
    }
}

impl PageNavigation {
    pub fn close_windows(&self) {
        for page in &self.pages {
            page.attach();
        }
    }
}

impl Page {
    fn detach(&self) {
        if self.toolbar.content().is_none() {
            self.host.remove(&self.content);
            self.toolbar.set_content(Some(&self.content));
            self.placeholder.set_visible(true);
        }
        self.window.present();
    }

    fn attach(&self) {
        if self.toolbar.content().is_some() {
            self.toolbar.set_content(gtk::Widget::NONE);
            self.host.append(&self.content);
            self.placeholder.set_visible(false);
        }
        self.window.set_visible(false);
    }
}
