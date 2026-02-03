CREATE TABLE positions (
    user_id UUID NOT NULL,
    symbol TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    average_price BIGINT NOT NULL,
    PRIMARY KEY (user_id, symbol)
);

CREATE INDEX idx_positions_user_id ON positions (user_id);
