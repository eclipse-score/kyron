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
from typing import Any

import pytest
from cit_runtime_scenario import Executable
from testing_utils.net import Address, create_connection


@pytest.fixture
def client_connection(connection_params: dict[str, Any], executable: Executable):
    """
    Create a TCP connection to the server.

    Parameters
    ----------
    connection : dict[str, Any]
        A dictionary containing the connection details with keys 'ip' and 'port'.
    executable : Executable
        The executable instance with running server.
    """

    executable.wait_for_log(
        lambda log_container: (
            log_container.find_log(
                "message",
                pattern=f"TCP server listening on {connection_params['ip']}:{connection_params['port']}",
            )
            is not None
        )
    )

    s = create_connection(Address.from_dict(connection_params), timeout=3.0)
    yield s
    s.close()
