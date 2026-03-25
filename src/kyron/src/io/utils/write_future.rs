// *******************************************************************************
// Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
// *******************************************************************************

use crate::macros::log::*;
use crate::{
    futures::{FutureInternalReturn, FutureState},
    io::AsyncWrite,
    not_recoverable_error,
};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use std::io::Error;

pub struct WriteFuture<'a, R: AsyncWrite + Unpin + ?Sized> {
    writer: &'a mut R,
    buf: &'a [u8],
    state: FutureState,
}

impl<'a, R: AsyncWrite + Unpin + ?Sized> WriteFuture<'a, R> {
    pub fn new(writer: &'a mut R, buf: &'a [u8]) -> Self {
        WriteFuture {
            writer,
            buf,
            state: FutureState::New,
        }
    }
}

impl<R: AsyncWrite + Unpin + ?Sized> Future for WriteFuture<'_, R> {
    type Output = Result<usize, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { writer, buf, state } = &mut *self;

        state.assign_and_propagate(match *state {
            FutureState::New | FutureState::Polled => match Pin::new(writer).poll_write(cx, buf) {
                Poll::Ready(poll_result) => FutureInternalReturn::ready(poll_result),
                Poll::Pending => FutureInternalReturn::polled(),
            },
            FutureState::Finished => {
                not_recoverable_error!("Future polled after it finished!")
            },
        })
    }
}
