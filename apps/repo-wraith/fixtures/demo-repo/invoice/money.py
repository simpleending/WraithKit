def to_cents(amount):
    """Convert a price in dollars to integer cents."""
    return int(amount * 100)   # bug: 19.99 * 100 == 1998.9999... -> 1998