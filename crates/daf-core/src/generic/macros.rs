//! Declarative composition macros for typed domains.
//! No proc macros, no recursion and no identifier synthesis.
//! Each invocation names its access/domain types and one monomorphic body.

#[macro_export]
macro_rules! daf_query {
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        run($pool_name:ident, $input_name:ident, $subject_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::QueryDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            async fn run(
                &self,
                $pool_name: &$pool,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
            ) -> ::core::result::Result<
                <Self as $crate::generic::QueryDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro query resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Debug, Clone, Default)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            pool: Option<$pool>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self { pool }
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.pool.as_ref()
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(&self, info: &$crate::generic::QueryInfo, subject: &$crate::generic::Subject) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                if info.resource != $resource {
                    return Err($crate::DataAccessError::Validation($crate::ValidationError::new(format!(
                        "the registered query adapter serves '{}', not '{}'",
                        $resource, info.resource
                    ))));
                }
                let Some(pool) = self.pool.as_ref() else {
                    return Err($crate::DataAccessError::Repository($crate::RepositoryError::new(format!(
                        "Postgres pool not configured; {} unavailable", $resource
                    ))));
                };
                let arguments = info.arguments.clone().unwrap_or_default();
                let input = <$domain as $crate::generic::QueryDomain<$pool>>::parse_arguments(&arguments).await?;
                let output = <$domain as $crate::generic::QueryDomain<$pool>>::run(&$domain, pool, input, subject).await?;
                ::serde_json::to_value(&output).map_err(|error| {
                    $crate::DataAccessError::Internal($crate::InternalError::new(format!(
                        "{} output encode: {error}", $resource
                    )))
                })
            }

            async fn execute_mutation(
                &self,
                info: &$crate::generic::MutationInfo,
                _subject: &$crate::generic::Subject,
                _context: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                Err($crate::DataAccessError::Authorization($crate::AuthorizationError::new(
                    format!("'{}' is a query — it serves no mutations", info.resource),
                )))
            }
        }
    };
}

#[macro_export]
macro_rules! daf_mutation {
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        idempotency = none,
        run($pool_name:ident, $input_name:ident, $subject_name:ident, $context_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::MutationDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            fn idempotency_key(&self, _subject: &$crate::generic::Subject, _input: &Self::Input) -> Option<String> {
                debug_assert!(!$resource.is_empty(), "idempotency names a resource");
                None
            }

            async fn run(
                &self,
                $pool_name: &$pool,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
                $context_name: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<
                <Self as $crate::generic::MutationDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro mutation resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Clone, Default)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            pool: Option<$pool>,
            idempotency: Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self { pool, idempotency: None }
            }

            #[must_use]
            pub fn with_idempotency(mut self, store: ::std::sync::Arc<dyn $crate::generic::Idempotency>) -> Self {
                debug_assert!(
                    ::std::mem::size_of::<Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>>() > 0,
                    "idempotency slot is representable"
                );
                self.idempotency = Some(store);
                self
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.pool.as_ref()
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(&self, info: &$crate::generic::QueryInfo, _subject: &$crate::generic::Subject) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                Err($crate::DataAccessError::Validation($crate::ValidationError::new(format!(
                    "'{}' is a mutation — it serves no queries", info.resource
                ))))
            }

            async fn execute_mutation(&self, info: &$crate::generic::MutationInfo, subject: &$crate::generic::Subject, context: &$crate::generic::MutationContext) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                if info.resource != $resource {
                    return Err($crate::DataAccessError::Validation($crate::ValidationError::new(format!(
                        "the registered mutation adapter serves '{}', not '{}'", $resource, info.resource
                    ))));
                }
                let Some(pool) = self.pool.as_ref() else {
                    return Err($crate::DataAccessError::Repository($crate::RepositoryError::new(format!(
                        "Postgres pool not configured; {} unavailable", $resource
                    ))));
                };
                let input = <$domain as $crate::generic::MutationDomain<$pool>>::parse_arguments(&info.arguments).await?;
                let output = <$domain as $crate::generic::MutationDomain<$pool>>::run(&$domain, pool, input, subject, context).await?;
                ::serde_json::to_value(&output).map_err(|error| {
                    $crate::DataAccessError::Internal($crate::InternalError::new(format!(
                        "{} output encode: {error}", $resource
                    )))
                })
            }
        }
    };
    (
        $(#[$access_meta:meta])* $access_vis:vis access $access:ident,
        $(#[$domain_meta:meta])* $domain_vis:vis domain $domain:ident,
        resource = $resource:literal,
        pool = $pool:ty,
        input = $input:ty,
        output = $output:ty,
        idempotency = subject_and_input,
        run($pool_name:ident, $input_name:ident, $subject_name:ident, $context_name:ident)
            $body:block
    ) => {
        $(#[$domain_meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $domain_vis struct $domain;

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::MutationDomain<$pool> for $domain {
            const RESOURCE: &'static str = $resource;
            type Input = $input;
            type Output = $output;

            fn idempotency_key(&self, $subject_name: &$crate::generic::Subject, $input_name: &Self::Input) -> Option<String> {
                debug_assert!(!$resource.is_empty(), "idempotency names a resource");
                Some($crate::generic::subject_json_key($subject_name, $input_name))
            }

            async fn run(
                &self,
                $pool_name: &$pool,
                $input_name: Self::Input,
                $subject_name: &$crate::generic::Subject,
                $context_name: &$crate::generic::MutationContext,
            ) -> ::core::result::Result<
                <Self as $crate::generic::MutationDomain<$pool>>::Output,
                $crate::DataAccessError,
            > {
                debug_assert!(!$resource.is_empty(), "macro mutation resources are named");
                $body
            }
        }

        $(#[$access_meta])*
        #[derive(Clone, Default)]
        #[allow(missing_docs)]
        $access_vis struct $access {
            pool: Option<$pool>,
            idempotency: Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>,
        }

        #[allow(missing_docs)]
        impl $access {
            #[must_use]
            pub const fn new(pool: Option<$pool>) -> Self {
                Self { pool, idempotency: None }
            }

            #[must_use]
            pub fn with_idempotency(mut self, store: ::std::sync::Arc<dyn $crate::generic::Idempotency>) -> Self {
                debug_assert!(
                    ::std::mem::size_of::<Option<::std::sync::Arc<dyn $crate::generic::Idempotency>>>() > 0,
                    "idempotency slot is representable"
                );
                self.idempotency = Some(store);
                self
            }

            #[must_use]
            pub const fn pool(&self) -> Option<&$pool> {
                self.pool.as_ref()
            }
        }

        #[doc(hidden)]
        #[allow(missing_docs)]
        #[::async_trait::async_trait]
        impl $crate::generic::DataAccess for $access {
            async fn query(&self, info: &$crate::generic::QueryInfo, _subject: &$crate::generic::Subject) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                Err($crate::DataAccessError::Validation($crate::ValidationError::new(format!(
                    "'{}' is a mutation — it serves no queries", info.resource
                ))))
            }

            async fn execute_mutation(&self, info: &$crate::generic::MutationInfo, subject: &$crate::generic::Subject, context: &$crate::generic::MutationContext) -> ::core::result::Result<serde_json::Value, $crate::DataAccessError> {
                if info.resource != $resource {
                    return Err($crate::DataAccessError::Validation($crate::ValidationError::new(format!(
                        "the registered mutation adapter serves '{}', not '{}'", $resource, info.resource
                    ))));
                }
                let Some(pool) = self.pool.as_ref() else {
                    return Err($crate::DataAccessError::Repository($crate::RepositoryError::new(format!(
                        "Postgres pool not configured; {} unavailable", $resource
                    ))));
                };
                let input = <$domain as $crate::generic::MutationDomain<$pool>>::parse_arguments(&info.arguments).await?;
                if let Some(store) = self.idempotency.as_ref() {
                    if let Some(key) = <$domain as $crate::generic::MutationDomain<$pool>>::idempotency_key(&$domain, subject, &input) {
                        if let Some(replayed) = store.replay(&key).await? {
                            return Ok(replayed);
                        }
                        let output = <$domain as $crate::generic::MutationDomain<$pool>>::run(&$domain, pool, input, subject, context).await?;
                        let encoded = ::serde_json::to_value(&output).map_err(|error| {
                            $crate::DataAccessError::Internal($crate::InternalError::new(format!(
                                "{} output encode: {error}", $resource
                            )))
                        })?;
                        store.record(&key, &encoded, None).await?;
                        return Ok(encoded);
                    }
                }
                let output = <$domain as $crate::generic::MutationDomain<$pool>>::run(&$domain, pool, input, subject, context).await?;
                ::serde_json::to_value(&output).map_err(|error| {
                    $crate::DataAccessError::Internal($crate::InternalError::new(format!(
                        "{} output encode: {error}", $resource
                    )))
                })
            }
        }
    };
}

#[macro_export]
macro_rules! daf_policy {
    (
        $(#[$meta:meta])* $policy_vis:vis policy struct $policy:ident;
        query { $( $query_resource:literal => $query_mode:ident ),+ $(,)? }
        $( mutation { $( $mutation_resource:literal => $mutation_mode:ident ),+ $(,)? } )?
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default)]
        $policy_vis struct $policy;

        impl $policy {
            #[must_use]
            pub fn policies() -> &'static [$crate::generic::ResourcePolicy] {
                debug_assert!(
                    ::std::mem::size_of::<Option<&'static str>>() > 0,
                    "resource identifiers are representable"
                );
                &[
                    $( $crate::generic::ResourcePolicy {
                        resource: $query_resource,
                        query: Some($crate::generic::AdmissionMode::$query_mode),
                        mutation: None,
                    }, )+
                    $( $( $crate::generic::ResourcePolicy {
                        resource: $mutation_resource,
                        query: None,
                        mutation: Some($crate::generic::AdmissionMode::$mutation_mode),
                    }, )* )?
                ]
            }
        }

        impl $crate::generic::PolicyTable for $policy {
            fn policies() -> &'static [$crate::generic::ResourcePolicy] {
                debug_assert!(
                    ::std::mem::size_of::<Option<&'static str>>() > 0,
                    "resource identifiers are representable"
                );
                $policy::policies()
            }
        }
    };
}

#[macro_export]
macro_rules! daf_rows {
    (
        $(#[$meta:meta])*
        $function_vis:vis fn $function:ident($pool_name:ident: Option<$pool:ty>) ->
            $returned:ty;
        $(row $resource:literal => $access:expr),+ $(,)?
    ) => {
        $(#[$meta])*
        $function_vis fn $function($pool_name: Option<$pool>) -> $returned {
            vec![
                $( $crate::generic::typed_access_row($resource, $access), )+
            ]
        }
    };
}

#[macro_export]
macro_rules! daf_row {
    ($resource:expr, access = $access:expr $(,)?) => {
        (
            $resource,
            ::std::sync::Arc::new($access) as ::std::sync::Arc<dyn $crate::generic::DataAccess>,
        )
    };
}
