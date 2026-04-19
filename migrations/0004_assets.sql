CREATE TYPE asset_class AS ENUM (
    'Stock',
    'Bond',
    'Commodity',
    'Alternative',
    'Crypto',
    'CashEquivalent'
);

CREATE TABLE assets (
    id            UUID PRIMARY KEY,
    isin          VARCHAR(12) NOT NULL UNIQUE,
    yahoo_ticker  VARCHAR,
    name          VARCHAR NOT NULL,
    asset_class   asset_class NOT NULL,
    currency      VARCHAR(10) NOT NULL,
    exchange      VARCHAR,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_assets_isin ON assets(isin);
CREATE INDEX idx_assets_asset_class ON assets(asset_class);
CREATE INDEX idx_assets_name_lower ON assets(LOWER(name));
