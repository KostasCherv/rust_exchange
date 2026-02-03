CREATE TABLE trades (
    id UUID PRIMARY KEY,
    maker_order_id UUID NOT NULL,
    taker_order_id UUID NOT NULL,
    maker_user_id UUID NOT NULL,
    taker_user_id UUID NOT NULL,
    symbol TEXT NOT NULL,
    price BIGINT NOT NULL,
    quantity BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_trades_maker_user_id ON trades (maker_user_id);
CREATE INDEX idx_trades_taker_user_id ON trades (taker_user_id);
CREATE INDEX idx_trades_symbol_created_at ON trades (symbol, created_at DESC);
