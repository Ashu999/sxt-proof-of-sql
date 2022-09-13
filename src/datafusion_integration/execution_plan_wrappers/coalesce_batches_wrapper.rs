use super::impl_provable_passthrough;
use crate::{
    base::{
        datafusion::{
            impl_debug_for_provable, impl_execution_plan_for_provable,
            DataFusionProof::{self, ExecutionPlanProof as ExecutionPlanProofEnumVariant},
            ExecutionPlanProof::TrivialProof as TrivialProofEnumVariant,
            Provable, ProvableExecutionPlan,
        },
        proof::{
            Commitment, IntoDataFusionResult, IntoProofResult, PipProve, PipVerify, ProofError,
            ProofResult, Table, Transcript,
        },
    },
    datafusion_integration::wrappers::{unwrap_exec_plan_if_wrapped, wrap_exec_plan},
    pip::execution_plan::TrivialProof,
};
use async_trait::async_trait;
use datafusion::{
    arrow::{datatypes::SchemaRef, record_batch::RecordBatch},
    execution::context::TaskContext,
    physical_plan::{
        coalesce_batches::CoalesceBatchesExec, common::collect, expressions::PhysicalSortExpr,
        metrics::MetricsSet, DisplayFormatType, Distribution, ExecutionPlan, Partitioning,
        SendableRecordBatchStream, Statistics,
    },
};
use std::sync::RwLock;
use std::{
    any::Any,
    fmt::{Debug, Formatter},
    stringify,
    sync::Arc,
};

pub struct CoalesceBatchesExecWrapper {
    raw: CoalesceBatchesExec,
    /// The input plan
    input: Arc<dyn ProvableExecutionPlan>,
    /// Same but as Arc<dyn ExecutionPlan> because trait upcast is unstable
    input_as_plan: Arc<dyn ExecutionPlan>,
    /// All the provables
    provable_children: Vec<Arc<dyn Provable>>,
    proof: RwLock<Option<Arc<DataFusionProof>>>,
    output: RwLock<Option<RecordBatch>>,
}

impl CoalesceBatchesExecWrapper {
    pub fn raw_spec(&self) -> CoalesceBatchesExec {
        CoalesceBatchesExec::new(self.raw.input().clone(), self.raw.target_batch_size())
    }

    pub fn try_new_from_raw(raw: &CoalesceBatchesExec) -> ProofResult<Self> {
        let raw_input = raw.input();
        let target_batch_size = raw.target_batch_size();
        let (wrapped_input, wrapped_input_as_plan, wrapped_input_as_provable) =
            wrap_exec_plan(raw_input)?;
        Ok(CoalesceBatchesExecWrapper {
            raw: CoalesceBatchesExec::new(raw_input.clone(), target_batch_size),
            input: wrapped_input.clone(),
            input_as_plan: wrapped_input_as_plan.clone(),
            provable_children: vec![wrapped_input_as_provable],
            proof: RwLock::new(None),
            output: RwLock::new(None),
        })
    }

    pub fn try_new_from_children(
        input: Arc<dyn ProvableExecutionPlan>,
        target_batch_size: usize,
    ) -> ProofResult<Self> {
        let raw = CoalesceBatchesExec::new(input.try_raw()?, target_batch_size);
        Self::try_new_from_raw(&raw)
    }

    /// The input plan
    pub fn input(&self) -> &Arc<dyn ProvableExecutionPlan> {
        &self.input
    }

    /// Minimum number of rows for coalesces batches
    pub fn target_batch_size(&self) -> usize {
        self.raw.target_batch_size()
    }
}

#[async_trait]
impl ProvableExecutionPlan for CoalesceBatchesExecWrapper {
    fn try_raw(&self) -> ProofResult<Arc<dyn ExecutionPlan>> {
        Ok(Arc::new(self.raw_spec()))
    }
    // Compute output of an execution plan and store it
    async fn execute_and_collect(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> ProofResult<()> {
        self.input
            .execute_and_collect(partition, context.clone())
            .await?;
        let stream: SendableRecordBatchStream = self
            .execute(partition, context.clone())
            .into_proof_result()?;
        let schema: SchemaRef = stream.schema();
        let output_batches = collect(stream).await.into_proof_result()?;
        let output = RecordBatch::concat(&schema, &output_batches[..]).into_proof_result()?;
        *self.output.write().into_proof_result()? = Some(output);
        Ok(())
    }
    // Return output of an execution plan
    fn output(&self) -> ProofResult<RecordBatch> {
        (*self.output.read().into_proof_result()?)
            .clone()
            .ok_or(ProofError::UnexecutedError)
    }
}

impl_provable_passthrough!(CoalesceBatchesExecWrapper);

impl ExecutionPlan for CoalesceBatchesExecWrapper {
    impl_execution_plan_for_provable!();
    fn children(&self) -> Vec<Arc<dyn ExecutionPlan>> {
        vec![self.input_as_plan.clone()]
    }
    fn output_ordering(&self) -> Option<&[PhysicalSortExpr]> {
        None
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> datafusion::common::Result<Arc<dyn ExecutionPlan>> {
        let input = children[0].clone();
        let raw_input = unwrap_exec_plan_if_wrapped(&input).into_datafusion_result()?;
        let raw = CoalesceBatchesExec::new(raw_input, self.raw.target_batch_size());
        Ok(Arc::new(
            CoalesceBatchesExecWrapper::try_new_from_raw(&raw).into_datafusion_result()?,
        ))
    }
}

impl_debug_for_provable!(CoalesceBatchesExecWrapper);
