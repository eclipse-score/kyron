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

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CommonErrors {
    NoData,
    AlreadyDone,
    GenericError,
    OperationAborted,
    Panicked, // panic! was handled
    Timeout,
    NoSpaceLeft,
    NotFound,
    WrongArgs,
    AlreadyExists,
    NotSupported,
}

impl From<CommonErrors> for std::io::Error {
    fn from(err: CommonErrors) -> Self {
        std::io::Error::from(match err {
            CommonErrors::NoData => std::io::ErrorKind::UnexpectedEof,
            CommonErrors::AlreadyDone => std::io::ErrorKind::AlreadyExists,
            CommonErrors::GenericError => std::io::ErrorKind::Other,
            CommonErrors::OperationAborted => std::io::ErrorKind::Interrupted,
            CommonErrors::Panicked => std::io::ErrorKind::Other,
            CommonErrors::Timeout => std::io::ErrorKind::TimedOut,
            CommonErrors::NoSpaceLeft => std::io::ErrorKind::OutOfMemory,
            CommonErrors::NotFound => std::io::ErrorKind::NotFound,
            CommonErrors::WrongArgs => std::io::ErrorKind::InvalidInput,
            CommonErrors::AlreadyExists => std::io::ErrorKind::AlreadyExists,
            CommonErrors::NotSupported => std::io::ErrorKind::Unsupported,
        })
    }
}
