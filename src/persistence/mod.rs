//! Database layer: pool, migrations, and access for users, orders, trades, positions.

mod orders;
mod pool;
mod positions;
mod trades;
mod users;

pub use orders::{
    get_order_by_id, insert_order, list_open_orders_by_symbol, order_row_to_order,
    order_row_to_order_display, update_order_status, OrderRow,
};
pub use pool::{create_pool_and_migrate, run_migrations};
pub use sqlx::PgPool;
pub use users::{get_user_by_username, insert_user, list_users};
pub use positions::{list_positions, list_positions_for_user, upsert_position, PositionRow};
pub use trades::{insert_trade, list_trades, list_trades_for_user};