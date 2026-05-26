use domain::ticket::TicketCategory;

#[derive(Debug, Clone)]
pub struct RaiseTicketCommand {
    pub title: String,
    pub description: String,
    pub category: TicketCategory,
}
