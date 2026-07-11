use domain::model::TicketCategory;

/// Input to raise an IT ticket.
#[derive(Debug, Clone)]
pub struct RaiseTicketCommand {
    pub title: String,
    pub description: String,
    pub category: TicketCategory,
}
