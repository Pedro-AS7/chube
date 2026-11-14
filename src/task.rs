//! Tarefas do app, no lugar do `Task` do libcosmic: o `App::handle` devolve o que fazer depois,
//! e o resultado volta como `Message` pelo canal do laço principal (`app::run`).
//!
//! Futuros rodam num runtime do tokio em threads próprias, longe da thread do GTK; esperas
//! simples usam o relógio do GLib.

use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;
use std::time::Duration;

use async_channel::Sender;

use crate::app::Message;

type BoxFuture = Pin<Box<dyn Future<Output = Message> + Send>>;

pub enum Task {
    None,
    Batch(Vec<Task>),
    Future(BoxFuture),
    /// Manda a mensagem depois da espera.
    After(Duration, Message),
}

impl Task {
    pub fn none() -> Self {
        Task::None
    }

    pub fn future(future: impl Future<Output = Message> + Send + 'static) -> Self {
        Task::Future(Box::pin(future))
    }

    /// Roda `work` numa thread de bloqueio (disco, processos) e manda a mensagem que ela devolve.
    pub fn blocking(work: impl FnOnce() -> Message + Send + 'static) -> Self {
        Task::future(async move {
            tokio::task::spawn_blocking(work)
                .await
                .unwrap_or_else(|error| Message::TaskFailed(error.to_string()))
        })
    }

    pub fn after(delay: Duration, message: Message) -> Self {
        Task::After(delay, message)
    }

    pub fn batch(tasks: impl IntoIterator<Item = Task>) -> Self {
        Task::Batch(tasks.into_iter().collect())
    }
}

/// Runtime do tokio para os futuros das tarefas e as conexões de estado da VPS.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("chube-tarefas")
            .enable_all()
            .build()
            .expect("runtime do tokio")
    });
    &RUNTIME
}

/// Começa a tarefa; o resultado chega em `tx`.
pub fn run(task: Task, tx: &Sender<Message>) {
    match task {
        Task::None => {}
        Task::Batch(tasks) => {
            for task in tasks {
                run(task, tx);
            }
        }
        Task::Future(future) => {
            let tx = tx.clone();
            runtime().spawn(async move {
                let _ = tx.send(future.await).await;
            });
        }
        Task::After(delay, message) => {
            let tx = tx.clone();
            gtk4::glib::timeout_add_local_once(delay, move || {
                let _ = tx.try_send(message);
            });
        }
    }
}
