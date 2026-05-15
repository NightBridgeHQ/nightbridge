from pathlib import Path


def test_python_example_stays_under_ten_logical_lines() -> None:
    example = Path(__file__).parents[3] / "docs" / "examples" / "python-list-peers-and-send.py"
    logical_lines = [
        line
        for line in example.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]

    assert len(logical_lines) <= 10
