CREATE TYPE transaction_type AS ENUM (
    'Buy',
    'Sell',
    'Dividend',
    'Coupon'
);

CREATE TABLE transactions (
    id                      UUID PRIMARY KEY,
    portfolio_id            UUID NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
    asset_id                UUID NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    transaction_type        transaction_type NOT NULL,
    date                    DATE NOT NULL,
    settlement_date         DATE,
    quantity                NUMERIC(18,8),
    unit_price              NUMERIC(18,8),
    commission              NUMERIC(18,4) NOT NULL DEFAULT 0,
    currency                VARCHAR(10) NOT NULL,
    exchange_rate_to_base   NUMERIC(18,8) NOT NULL,
    gross_amount            NUMERIC(18,4),
    tax_withheld            NUMERIC(18,4),
    net_amount              NUMERIC(18,4),
    notes                   TEXT,
    import_hash             VARCHAR,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_transactions_portfolio_asset ON transactions(portfolio_id, asset_id);
CREATE INDEX idx_transactions_portfolio_date ON transactions(portfolio_id, date);
