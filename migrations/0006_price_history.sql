CREATE TABLE price_history (
    id           UUID PRIMARY KEY,
    asset_id     UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    date         DATE NOT NULL,
    close_price  NUMERIC(18,8) NOT NULL,
    currency     VARCHAR(10) NOT NULL,
    source       VARCHAR NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(asset_id, date)
);

CREATE INDEX idx_price_history_asset_id ON price_history(asset_id);
CREATE INDEX idx_price_history_asset_date ON price_history(asset_id, date DESC);
