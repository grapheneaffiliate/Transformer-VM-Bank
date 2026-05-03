"""Pytest fixtures and CLI options for the bit-exact gate harness."""

import pytest


def pytest_addoption(parser):
    parser.addoption(
        "--full",
        action="store_true",
        help="Run all 10k vectors per primitive (default: 100, --quick set)",
    )


@pytest.fixture(scope="session")
def vector_count(pytestconfig):
    return None if pytestconfig.getoption("--full") else 100
