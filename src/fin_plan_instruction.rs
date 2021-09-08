use fin_plan::FinPlan;
use chrono::prelude::{DateTime, Utc};


#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Contract {

    pub tokens: i64,
    pub fin_plan: FinPlan,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Vote {
    pub version: u64,
    pub contact_info_version: u64,
}


#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Instruction {
    
    NewContract(Contract),

    
    ApplyTimestamp(DateTime<Utc>),

    
    ApplySignature,

    
    NewVote(Vote),
}
