# *******************************************************************************
# Copyright (c) 2025 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0
# *******************************************************************************
from socket import SOCK_DGRAM, socket
from typing import Any

import pytest
from cit_runtime_scenario import Executable
from testing_utils.net import Address


@pytest.fixture
def udp_client(connection_params: dict[str, Any], executable: Executable):
    """
    Create an UDP socket to communicate with the server.

    Parameters
    ----------
    connection_params : dict[str, Any]
        A dictionary containing the connection details with keys 'ip' and 'port'.
    executable : Executable
        The executable instance with running server.
    """

    executable.wait_for_log(
        lambda log_container: (
            log_container.find_log(
                "message",
                pattern=f"UDP server listener running on {connection_params['ip']}:{connection_params['port']}",
            )
            is not None
        )
    )

    return socket(Address.from_dict(connection_params).family(), SOCK_DGRAM)
