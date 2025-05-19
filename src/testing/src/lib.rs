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

pub mod mock_fn;
pub mod poller;
pub mod prelude;
pub mod waker;

pub fn assert_poll_ready<T: PartialEq + std::fmt::Debug>(res: std::task::Poll<T>, val: T) {
    match res {
        std::task::Poll::Ready(v) => assert_eq!(val, v),
        std::task::Poll::Pending => panic!("Shall have Ready value and not Pending"),
    }
}
