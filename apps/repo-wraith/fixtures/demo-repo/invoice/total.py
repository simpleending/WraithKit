from invoice.money import to_cents


def invoice_total(lines):
    """Sum line items (name, quantity, unit price in dollars) in cents."""
    return sum(to_cents(price) * qty for _, qty, price in lines)