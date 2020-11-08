use crate::logging::NoContextLog;
use crate::move_vm::MoveVM;
use crate::{
    data_cache::{RemoteCache, TransactionEffects},
    session::Session,
};
use libra_logger::prelude::*;
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
};
use move_vm_types::data_store::DataStore;
use move_vm_types::{gas_schedule::CostStrategy, values::Value};
use vm::errors::*;
use vm::CompiledModule;

/// A adapter for wrap MoveVM
pub struct MoveVMAdapter {
    vm: MoveVM,
}

impl MoveVMAdapter {
    pub fn new() -> Self {
        Self { vm: MoveVM::new() }
    }

    pub fn new_session<'r, R: RemoteCache>(&self, remote: &'r R) -> SessionAdapter<'r, '_, R> {
        SessionAdapter::new(self.vm.new_session(remote))
    }
}

pub struct SessionAdapter<'r, 'l, R> {
    pub(crate) session: Session<'r, 'l, R>,
    log_context: NoContextLog,
}

impl<'r, 'l, R: RemoteCache> SessionAdapter<'r, 'l, R> {
    pub fn new(session: Session<'r, 'l, R>) -> Self {
        Self {
            session,
            log_context: NoContextLog::new(),
        }
    }

    pub fn execute_function(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<TypeTag>,
        args: Vec<Value>,
        sender: AccountAddress,
        cost_strategy: &mut CostStrategy,
    ) -> VMResult<()> {
        self.session.execute_function(
            module,
            function_name,
            ty_args,
            args,
            sender,
            cost_strategy,
            &self.log_context,
        )
    }

    pub fn execute_readonly_function(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<TypeTag>,
        args: Vec<Value>,
        cost_strategy: &mut CostStrategy,
    ) -> VMResult<Vec<(TypeTag, Value)>> {
        self.session.runtime.execute_readonly_function(
            module,
            function_name,
            ty_args,
            args,
            &mut self.session.data_cache,
            cost_strategy,
            &self.log_context,
        )
    }

    pub fn execute_script(
        &mut self,
        script: Vec<u8>,
        ty_args: Vec<TypeTag>,
        args: Vec<Value>,
        senders: Vec<AccountAddress>,
        cost_strategy: &mut CostStrategy,
    ) -> VMResult<()> {
        self.session.execute_script(
            script,
            ty_args,
            args,
            senders,
            cost_strategy,
            &self.log_context,
        )
    }

    pub fn publish_module(
        &mut self,
        module: Vec<u8>,
        sender: AccountAddress,
        cost_strategy: &mut CostStrategy,
    ) -> VMResult<()> {
        self.session
            .publish_module(module, sender, cost_strategy, &self.log_context)
    }

    pub fn verify_module(&mut self, module: &[u8]) -> VMResult<CompiledModule> {
        let compiled_module = match CompiledModule::deserialize(module) {
            Ok(module) => module,
            Err(err) => {
                warn!("[VM] module deserialization failed {:?}", err);
                return Err(err.finish(Location::Undefined));
            }
        };
        self.session
            .runtime
            .loader()
            .verify_module_verify_no_missing_dependencies(
                &compiled_module,
                &mut self.session.data_cache,
                &self.log_context,
            )?;
        Ok(compiled_module)
    }

    pub fn exists_module(&self, module_id: &ModuleId) -> VMResult<bool> {
        self.session.data_cache.exists_module(module_id)
    }

    pub fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        self.session.data_cache.load_module(module_id)
    }

    pub fn num_mutated_accounts(&self) -> u64 {
        self.session.num_mutated_accounts()
    }

    pub fn finish(self) -> VMResult<TransactionEffects> {
        self.session.finish()
    }
}