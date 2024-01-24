use std::{cell::RefCell, process::exit};

use futures::{Stream, StreamExt};
use gtk::glib;
use gtk::prelude::*;
use webkit::{prelude::*, CookieManager, WebView};

fn main() -> glib::ExitCode {
    let app = gtk::Application::new(
        Some("com.github.polymeilex.fortitude-webview"),
        gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE,
    );
    app.connect_command_line(|app, cmd| {
        let args = cmd.arguments();
        let url = args
            .get(1)
            .expect("Missing FORTIVPN_URL arg")
            .to_str()
            .expect("Non UTF8 url");
        build_ui(app, url);
        0
    });
    app.run()
}

fn build_ui(app: &gtk::Application, url: &str) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .default_width(400)
        .default_height(500)
        .build();

    let url = url.to_string();
    glib::MainContext::default().spawn_local(async move {
        let cookie = open_webview(window, &url);
        let res = cookie.await.unwrap();
        println!("{res}");
        exit(0);
    });
}

async fn open_webview(window: gtk::ApplicationWindow, base_url: &str) -> Option<String> {
    let realm = "foo";
    let realm = format!("?realm={realm}");
    let url = format!("https://{base_url}/remote/saml/start{realm}");

    let webview = WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    window.set_child(Some(&webview));

    let network_session = webview.network_session().unwrap();
    let cookie_manager = network_session.cookie_manager().unwrap();

    cookie_manager.set_persistent_storage("./stogage.data", webkit::CookiePersistentStorage::Text);

    let mut uri_notify = webview_uri_notify(&webview);

    webview.load_uri(&url);
    window.present();

    let res = loop {
        let Some(uri) = uri_notify.next().await else {
            break None;
        };

        if let Some(cookie) = get_svpn_cookie(uri.as_str(), &cookie_manager).await {
            break Some(cookie);
        }
    };

    window.close();
    res
}

async fn get_svpn_cookie(uri: &str, cookie_manager: &CookieManager) -> Option<String> {
    if !uri.contains("sslvpn/portal.html") {
        return None;
    }

    let host = uri.split("sslvpn/portal.html").next()?;
    let cookies = cookie_manager.cookies_future(host).await.ok()?;

    let mut cookie = cookies.into_iter().find_map(|mut c| {
        let name = c.name()?;
        (name.as_str() == "SVPNCOOKIE").then_some(c)
    })?;

    let value = cookie.value()?;
    Some(format!("SVPNCOOKIE={}", value))
}

fn webview_uri_notify(webview: &WebView) -> impl Stream<Item = String> {
    let (tx, rx) = futures::channel::mpsc::channel::<String>(10);

    let tx = RefCell::new(tx);
    webview.connect_uri_notify({
        move |webview| {
            if let Some(uri) = webview.uri() {
                tx.borrow_mut().try_send(uri.to_string()).unwrap();
            }
        }
    });

    rx
}
