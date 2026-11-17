from invoice.money import to_cents


def test_to_cents_rounds_floats():
    assert to_cents(19.99) == 1999