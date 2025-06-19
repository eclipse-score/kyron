//
// Copyright (c) 2025 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
//

use async_runtime::futures::sleep;
use async_runtime::prelude::*;
use async_runtime::time::clock::Clock;
use async_runtime::{
    runtime::async_runtime::AsyncRuntimeBuilder,
    safety::{self, ensure_safety_enabled},
    scheduler::execution_engine::*,
    spawn, spawn_on_dedicated,
};

use foundation::prelude::*;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct X {}

impl Future for X {
    type Output = ();

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        cx.waker().wake_by_ref();

        std::task::Poll::Ready(())
    }
}

async fn ensure_sleep_correct(duration: Duration, storage: Arc<Mutex<HashMap<Duration, std::vec::Vec<Duration>>>>) {
    let now = Clock::now();
    sleep::sleep(duration).await;

    let dur = now.elapsed();
    error!("Slept for: {:?}, requested {:?}", dur, duration);

    let mut storage = storage.lock().unwrap();
    storage.entry(duration).or_insert(vec![]).push(dur);

    // assert!(
    //     dur >= duration && dur <= (duration + Duration::from_millis(((dur.as_millis() as f64 * 0.1) as u64).max(2))),
    //     "Sleep did not last long enough, reuqested: {:?}, actual: {:?}",
    //     duration,
    //     dur
    // );
}

use plotters::prelude::*;
// use rand::thread_rng;
// use rand_distr::{Distribution, Normal};

fn main() {
    tracing_subscriber::fmt()
        // .with_span_events(FmtSpan::FULL) // Ensures span open/close events are logged
        .with_target(false) // Optional: Remove module path
        .with_max_level(Level::DEBUG)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();

    let (builder, _engine_id) = AsyncRuntimeBuilder::new().with_engine(
        ExecutionEngineBuilder::new()
            .task_queue_size(256)
            .workers(3)
            .with_dedicated_worker("dedicated".into())
            .enable_safety_worker(ThreadParameters::default()),
    );
    let mut runtime = builder.build().unwrap();

    let o = Arc::new(Mutex::new(HashMap::new()));
    let storage = o.clone();

    let _ = runtime.block_on(async move {
        ensure_safety_enabled();
        let mut i = 0;
        let s_clone = storage.clone();

        while i < 200 {
            i += 1;
            // TASK
            error!("We do have first enter into runtime ;)");
            let storage = s_clone.clone();
            let handle = spawn(async move {
                // TASK
                error!("And again from one we are in another ;)");

                let s_clone = storage.clone();

                let _ = safety::spawn(async move {
                    //TASK
                    error!("And again from one nested from dedicated ;)");

                    ensure_sleep_correct(std::time::Duration::from_millis(13), storage.clone()).await;
                    warn!("After 13 ms sleep)");
                    ensure_sleep_correct(std::time::Duration::from_millis(423), storage.clone()).await;
                    warn!("Before 678 ms sleep");
                    ensure_sleep_correct(std::time::Duration::from_millis(324), storage.clone()).await;

                    warn!("After 678 ms sleep");

                    Err(0) as Result<i32, i32>
                })
                .await
                .unwrap()
                .is_err();

                let x = "dedicated".into();

                error!("I RUN FROM SAFETY FROM NOW ON !!!");

                spawn_on_dedicated(
                    async move {
                        // TASK
                        error!("I AM DEDICATED  ;)");
                        ensure_sleep_correct(std::time::Duration::from_millis(89), s_clone.clone()).await;
                        error!("I AM DEDICATED RESTORED  ;)");
                        1
                    },
                    x,
                )
                .await
                .unwrap();

                1
            });

            ensure_sleep_correct(std::time::Duration::from_millis(123), s_clone.clone()).await;

            let res = handle.await;
            error!("After await res is {}", res.unwrap());

            ensure_sleep_correct(std::time::Duration::from_secs(1), s_clone.clone()).await;

            error!("After sleep");
        }

        Ok(0)
    });

    let s = o.lock().unwrap();

    s.iter().for_each(|(k, v)| {
        let cor = k.as_millis() as f64;
        let samples: std::vec::Vec<f64> = v.iter().map(|d| d.as_millis() as f64).collect();

        // Plot
        let path = format!("dot_distribution_{}.png", cor);

        let root = BitMapBackend::new(path.as_str(), (1280, 800)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.margin(20, 20, 20, 20);

        // let x_range = (cor - 20.0)..(cor + 20.0);
        let x_min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let x_max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let mut chart = ChartBuilder::on(&root)
            .caption("Dot Distribution Around cor", ("sans-serif", 30))
            .x_label_area_size(40)
            .y_label_area_size(30)
            .build_cartesian_2d(x_min..x_max, 0..samples.len())
            .unwrap();

        chart.configure_mesh().disable_y_mesh().disable_x_mesh().x_desc("Value").draw().unwrap();

        // Plot each sample as a dot
        chart
            .draw_series(samples.iter().enumerate().map(|(i, &x)| Circle::new((x, i), 2, RED.filled())))
            .unwrap();

        // Optional: Draw a vertical line at cor
        chart.draw_series(LineSeries::new(vec![(cor, 0), (cor, samples.len())], &BLUE)).unwrap();

        println!("Saved to dot_distribution.png");
    });
}
