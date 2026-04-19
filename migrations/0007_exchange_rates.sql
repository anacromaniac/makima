CREATE TABLE exchange_rates (
    id              UUID PRIMARY KEY,
    from_currency   VARCHAR(10) NOT NULL,
    to_currency     VARCHAR(10) NOT NULL,
    date            DATE NOT NULL,
    rate            NUMERIC(18,8) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(from_currency, to_currency, date)
);

CREATE INDEX idx_exchange_rates_pair_date
    ON exchange_rates(from_currency, to_currency, date DESC);
