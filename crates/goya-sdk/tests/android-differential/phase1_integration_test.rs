//! Phase 1 Integration Testing
//! 
//! This module provides comprehensive integration testing for the complete
//! Phase 1 Android Emitter Implementation. It validates that all enhanced
//! Android emission features work correctly together and with existing functionality.

use crate::android_input_preview::AndroidRootInputPreviewError;
use crate::android_scrollable_preview::AndroidScrollablePreviewError;
use crate::android_d1_runtime_enhanced::AndroidD1RuntimeError;
use std::fs;
use std::path::Path;

/// Phase 1 integration test configuration
struct Phase1IntegrationConfig {
    enable_input_preview_tests: bool,
    enable_scrollable_preview_tests: bool,
    enable_d1_runtime_tests: bool,
    validate_backward_compatibility: bool,
    test_complex_scenarios: bool,
}

/// Phase 1 integration test result
#[derive(Debug, Clone)]
struct Phase1IntegrationTestResult {
    test_category: TestCategory,
    passed: usize,
    failed: usize,
    warnings: usize,
    total: usize,
    execution_time: std::time::Duration,
    details: Vec<TestResultDetail>,
}

/// Test category enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestCategory {
    EnhancedInputPreview,
    EnhancedScrollablePreview,
    EnhancedD1Runtime,
    BackwardCompatibility,
    ComplexScenarios,
}

/// Test result detail
#[derive(Debug, Clone)]
struct TestResultDetail {
    test_name: String,
    test_category: TestCategory,
    result: TestResult,
    execution_time: std::time::Duration,
    error_message: Option<String>,
    kotlin_output: Option<String>,
}

/// Test result enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestResult {
    Pass,
    Fail,
    Warning,
}

/// Phase 1 integration test orchestrator
struct Phase1IntegrationTestOrchestrator {
    config: Phase1IntegrationConfig,
    results: Vec<Phase1IntegrationTestResult>,
}

impl Phase1IntegrationTestOrchestrator {
    /// Create a new integration test orchestrator
    fn new(config: Phase1IntegrationConfig) -> Self {
        Self {
            config,
            results: Vec::new(),
        }
    }
    
    /// Run complete Phase 1 integration tests
    fn run_all_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting Phase 1 Integration Tests");
        println!("Testing complete Android Emitter Implementation");
        
        // Run enhanced input preview tests
        if self.config.enable_input_preview_tests {
            let result = self.run_enhanced_input_preview_tests()?;
            self.results.push(result);
        }
        
        // Run enhanced scrollable preview tests
        if self.config.enable_scrollable_preview_tests {
            let result = self.run_enhanced_scrollable_preview_tests()?;
            self.results.push(result);
        }
        
        // Run enhanced D1 runtime tests
        if self.config.enable_d1_runtime_tests {
            let result = self.run_enhanced_d1_runtime_tests()?;
            self.results.push(result);
        }
        
        // Run backward compatibility tests
        if self.config.validate_backward_compatibility {
            let result = self.run_backward_compatibility_tests()?;
            self.results.push(result);
        }
        
        // Run complex scenario tests
        if self.config.test_complex_scenarios {
            let result = self.run_complex_scenario_tests()?;
            self.results.push(result);
        }
        
        // Generate integration report
        self.generate_integration_report()?;
        
        // Check if integration tests passed
        let total_passed: usize = self.results.iter().map(|r| r.passed).sum();
        let total_failed: usize = self.results.iter().map(|r| r.failed).sum();
        
        if total_failed > 0 {
            return Err(format!("Phase 1 integration tests failed: {} tests failed", total_failed).into());
        }
        
        println!("\n✅ Phase 1 Integration Tests Completed Successfully!");
        Ok(())
    }
    
    /// Run enhanced input preview tests
    fn run_enhanced_input_preview_tests(&mut self) -> Result<Phase1IntegrationTestResult, Box<dyn std::error::Error>> {
        println!("\n=== Testing Enhanced Input Preview Features ===");
        
        let start_time = std::time::Instant::now();
        let mut test_details = Vec::new();
        
        // Test 1: Basic input preview functionality
        let test1_start = std::time::Instant::now();
        let basic_input_source = r#"@ontology demo {
            @cell username {
                id: "username";
                text: "Enter username";
                max-length: 50;
            }
            @cell password {
                id: "password";
                text: "Enter password";
                max-length: 20;
            }
            @cell email {
                id: "email";
                text: "Enter email";
                max-length: 100;
            }
        }"#;
        
        let kotlin_result = match crate::android_input_preview::emit_android_root_input_preview(basic_input_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Basic Input Preview".to_string(),
                    test_category: TestCategory::EnhancedInputPreview,
                    result: TestResult::Fail,
                    execution_time: test1_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test1_result = if !kotlin_result.is_empty() && kotlin_result.contains("GoyaRootInputPreview") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Basic Input Preview".to_string(),
            test_category: TestCategory::EnhancedInputPreview,
            result: test1_result,
            execution_time: test1_start.elapsed(),
            error_message: if test1_result == TestResult::Fail { Some("Input preview generation failed".to_string()) } else { None },
            kotlin_output: if test1_result == TestResult::Pass { Some(kotlin_result.clone()) } else { None },
        });
        
        // Test 2: Input preview with IME integration
        let test2_start = std::time::Instant::now();
        let ime_input_source = r#"@ontology demo {
            @cell username {
                id: "username";
                text: "John Doe";
                max-length: 30;
                placeholder: "Enter your name";
            }
        }"#;
        
        let kotlin_result_2 = match crate::android_input_preview::emit_android_root_input_preview(ime_input_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "IME Integration Input Preview".to_string(),
                    test_category: TestCategory::EnhancedInputPreview,
                    result: TestResult::Fail,
                    execution_time: test2_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test2_result = if !kotlin_result_2.is_empty() && kotlin_result_2.contains("GoyaRootInputPreview") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "IME Integration Input Preview".to_string(),
            test_category: TestCategory::EnhancedInputPreview,
            result: test2_result,
            execution_time: test2_start.elapsed(),
            error_message: if test2_result == TestResult::Fail { Some("IME integration test failed".to_string()) } else { None },
            kotlin_output: if test2_result == TestResult::Pass { Some(kotlin_result_2.clone()) } else { None },
        });
        
        // Test 3: Named class input preview
        let test3_start = std::time::Instant::now();
        let named_input_source = r#"@ontology demo {
            @cell username {
                id: "username";
                text: "Test User";
            }
        }"#;
        
        let kotlin_result_3 = match crate::android_input_preview::emit_android_root_input_preview_named(named_input_source, "CustomInputPreview") {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Named Class Input Preview".to_string(),
                    test_category: TestCategory::EnhancedInputPreview,
                    result: TestResult::Fail,
                    execution_time: test3_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test3_result = if !kotlin_result_3.is_empty() && kotlin_result_3.contains("CustomInputPreview") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Named Class Input Preview".to_string(),
            test_category: TestCategory::EnhancedInputPreview,
            result: test3_result,
            execution_time: test3_start.elapsed(),
            error_message: if test3_result == TestResult::Fail { Some("Named class test failed".to_string()) } else { None },
            kotlin_output: if test3_result == TestResult::Pass { Some(kotlin_result_3.clone()) } else { None },
        });
        
        // Calculate test results
        let passed = test_details.iter().filter(|d| d.result == TestResult::Pass).count();
        let failed = test_details.iter().filter(|d| d.result == TestResult::Fail).count();
        let warnings = test_details.iter().filter(|d| d.result == TestResult::Warning).count();
        let total = test_details.len();
        
        println!("Enhanced Input Preview Tests: {} passed, {} failed, {} warnings", passed, failed, warnings);
        
        Ok(Phase1IntegrationTestResult {
            test_category: TestCategory::EnhancedInputPreview,
            passed,
            failed,
            warnings,
            total,
            execution_time: start_time.elapsed(),
            details: test_details,
        })
    }
    
    /// Run enhanced scrollable preview tests
    fn run_enhanced_scrollable_preview_tests(&mut self) -> Result<Phase1IntegrationTestResult, Box<dyn std::error::Error>> {
        println!("\n=== Testing Enhanced Scrollable Preview Features ===");
        
        let start_time = std::time::Instant::now();
        let mut test_details = Vec::new();
        
        // Test 1: Basic scrollable preview
        let test1_start = std::time::Instant::now();
        let basic_scroll_source = r#"@ontology demo {
            @cell viewport {
                id: "viewport";
                scroll: true;
                @cell content {
                    id: "content";
                    extent: (800, 600, 0);
                }
            }
        }"#;
        
        let kotlin_result = match crate::android_scrollable_preview::emit_android_scrollable_preview(basic_scroll_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Basic Scrollable Preview".to_string(),
                    test_category: TestCategory::EnhancedScrollablePreview,
                    result: TestResult::Fail,
                    execution_time: test1_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test1_result = if !kotlin_result.is_empty() && kotlin_result.contains("GoyaScrollablePreview") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Basic Scrollable Preview".to_string(),
            test_category: TestCategory::EnhancedScrollablePreview,
            result: test1_result,
            execution_time: test1_start.elapsed(),
            error_message: if test1_result == TestResult::Fail { Some("Basic scrollable preview failed".to_string()) } else { None },
            kotlin_output: if test1_result == TestResult::Pass { Some(kotlin_result.clone()) } else { None },
        });
        
        // Calculate test results
        let passed = test_details.iter().filter(|d| d.result == TestResult::Pass).count();
        let failed = test_details.iter().filter(|d| d.result == TestResult::Fail).count();
        let warnings = test_details.iter().filter(|d| d.result == TestResult::Warning).count();
        let total = test_details.len();
        
        println!("Enhanced Scrollable Preview Tests: {} passed, {} failed, {} warnings", passed, failed, warnings);
        
        Ok(Phase1IntegrationTestResult {
            test_category: TestCategory::EnhancedScrollablePreview,
            passed,
            failed,
            warnings,
            total,
            execution_time: start_time.elapsed(),
            details: test_details,
        })
    }
    
    /// Run enhanced D1 runtime tests
    fn run_enhanced_d1_runtime_tests(&mut self) -> Result<Phase1IntegrationTestResult, Box<dyn std::error::Error>> {
        println!("\n=== Testing Enhanced D1 Runtime Features ===");
        
        let start_time = std::time::Instant::now();
        let mut test_details = Vec::new();
        
        // Test 1: Basic D1 runtime with form
        let test1_start = std::time::Instant::now();
        let basic_d1_source = r#"@ontology demo {
            @cell username {
                id: "username";
                text: "Username";
            }
            @interaction validate_username {
                trigger: button.submit.activate;
                class: D1;
                state {
                    read user_count: int;
                }
                input {
                    username: text;
                }
                set user_count = state.user_count + 1;
            }
            @button submit {
                id: "submit";
                text: "Submit";
            }
        }"#;
        
        let kotlin_result = match crate::android_d1_runtime_enhanced::emit_android_d1_runtime_with_form(basic_d1_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Basic D1 Runtime with Form".to_string(),
                    test_category: TestCategory::EnhancedD1Runtime,
                    result: TestResult::Fail,
                    execution_time: test1_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test1_result = if !kotlin_result.is_empty() && kotlin_result.contains("GoyaD1RuntimeForm") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Basic D1 Runtime with Form".to_string(),
            test_category: TestCategory::EnhancedD1Runtime,
            result: test1_result,
            execution_time: test1_start.elapsed(),
            error_message: if test1_result == TestResult::Fail { Some("Basic D1 runtime failed".to_string()) } else { None },
            kotlin_output: if test1_result == TestResult::Pass { Some(kotlin_result.clone()) } else { None },
        });
        
        // Test 2: Named class D1 runtime
        let test2_start = std::time::Instant::now();
        let named_d1_source = r#"@ontology demo {
            @cell username {
                id: "username";
                text: "Username";
            }
            @interaction validate_username {
                trigger: button.submit.activate;
                class: D1;
                state {
                    read user_count: int;
                }
                input {
                    username: text;
                }
                set user_count = state.user_count + 1;
            }
            @button submit {
                id: "submit";
                text: "Submit";
            }
        }"#;
        
        let kotlin_result_2 = match crate::android_d1_runtime_enhanced::emit_android_d1_runtime_with_form_named(named_d1_source, "MyCustomFormRuntime") {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Named Class D1 Runtime".to_string(),
                    test_category: TestCategory::EnhancedD1Runtime,
                    result: TestResult::Fail,
                    execution_time: test2_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test2_result = if !kotlin_result_2.is_empty() && kotlin_result_2.contains("MyCustomFormRuntime") {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Named Class D1 Runtime".to_string(),
            test_category: TestCategory::EnhancedD1Runtime,
            result: test2_result,
            execution_time: test2_start.elapsed(),
            error_message: if test2_result == TestResult::Fail { Some("Named class D1 runtime failed".to_string()) } else { None },
            kotlin_output: if test2_result == TestResult::Pass { Some(kotlin_result_2.clone()) } else { None },
        });
        
        // Calculate test results
        let passed = test_details.iter().filter(|d| d.result == TestResult::Pass).count();
        let failed = test_details.iter().filter(|d| d.result == TestResult::Fail).count();
        let warnings = test_details.iter().filter(|d| d.result == TestResult::Warning).count();
        let total = test_details.len();
        
        println!("Enhanced D1 Runtime Tests: {} passed, {} failed, {} warnings", passed, failed, warnings);
        
        Ok(Phase1IntegrationTestResult {
            test_category: TestCategory::EnhancedD1Runtime,
            passed,
            failed,
            warnings,
            total,
            execution_time: start_time.elapsed(),
            details: test_details,
        })
    }
    
    /// Run backward compatibility tests
    fn run_backward_compatibility_tests(&mut self) -> Result<Phase1IntegrationTestResult, Box<dyn std::error::Error>> {
        println!("\n=== Testing Backward Compatibility ===");
        
        let start_time = std::time::Instant::now();
        let mut test_details = Vec::new();
        
        // Test 1: Static text preview (original functionality)
        let test1_start = std::time::Instant::now();
        let static_text_source = r#"@ontology demo {
            @cell label {
                id: "label";
                text: "Hello World";
                text-size: 18;
                fill: #FF0000;
            }
        }"#;
        
        let kotlin_result = match crate::android_input_preview::emit_android_root_input_preview(static_text_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Static Text Preview Compatibility".to_string(),
                    test_category: TestCategory::BackwardCompatibility,
                    result: TestResult::Fail,
                    execution_time: test1_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test1_result = if !kotlin_result.is_empty() {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Static Text Preview Compatibility".to_string(),
            test_category: TestCategory::BackwardCompatibility,
            result: test1_result,
            execution_time: test1_start.elapsed(),
            error_message: if test1_result == TestResult::Fail { Some("Backward compatibility test failed".to_string()) } else { None },
            kotlin_output: if test1_result == TestResult::Pass { Some(kotlin_result.clone()) } else { None },
        });
        
        // Calculate test results
        let passed = test_details.iter().filter(|d| d.result == TestResult::Pass).count();
        let failed = test_details.iter().filter(|d| d.result == TestResult::Fail).count();
        let warnings = test_details.iter().filter(|d| d.result == TestResult::Warning).count();
        let total = test_details.len();
        
        println!("Backward Compatibility Tests: {} passed, {} failed, {} warnings", passed, failed, warnings);
        
        Ok(Phase1IntegrationTestResult {
            test_category: TestCategory::BackwardCompatibility,
            passed,
            failed,
            warnings,
            total,
            execution_time: start_time.elapsed(),
            details: test_details,
        })
    }
    
    /// Run complex scenario tests
    fn run_complex_scenario_tests(&mut self) -> Result<Phase1IntegrationTestResult, Box<dyn std::error::Error>> {
        println!("\n=== Testing Complex Scenarios ===");
        
        let start_time = std::time::Instant::now();
        let mut test_details = Vec::new();
        
        // Test 1: Complex form with multiple fields
        let test1_start = std::time::Instant::now();
        let complex_form_source = r#"@ontology demo {
            @cell first_name {
                id: "first_name";
                text: "John";
                max-length: 30;
                text-size: 18;
                fill: #4A90E2;
            }
            @cell last_name {
                id: "last_name";
                text: "Doe";
                max-length: 30;
                text-size: 18;
                fill: #4A90E2;
            }
            @cell email {
                id: "email";
                text: "john.doe@example.com";
                placeholder: "john.doe@example.com";
                max-length: 100;
                text-size: 16;
                fill: #E0E0E0;
            }
            @cell phone_number {
                id: "phone_number";
                text: "(555) 123-4567";
                placeholder: "(555) 123-4567";
                max-length: 20;
            }
            @cell address {
                id: "address";
                text: "123 Main Street, New York, NY 10001";
                placeholder: "123 Main Street, City, State ZIP";
                max-length: 200;
                text-size: 14;
            }
        }"#;
        
        let kotlin_result = match crate::android_input_preview::emit_android_root_input_preview(complex_form_source) {
            Ok(result) => result,
            Err(e) => {
                test_details.push(TestResultDetail {
                    test_name: "Complex Form Integration".to_string(),
                    test_category: TestCategory::ComplexScenarios,
                    result: TestResult::Fail,
                    execution_time: test1_start.elapsed(),
                    error_message: Some(format!("Error: {}", e)),
                    kotlin_output: None,
                });
                String::new()
            }
        };
        
        let test1_result = if !kotlin_result.is_empty() {
            TestResult::Pass
        } else {
            TestResult::Fail
        };
        
        test_details.push(TestResultDetail {
            test_name: "Complex Form Integration".to_string(),
            test_category: TestCategory::ComplexScenarios,
            result: test1_result,
            execution_time: test1_start.elapsed(),
            error_message: if test1_result == TestResult::Fail { Some("Complex form integration test failed".to_string()) } else { None },
            kotlin_output: if test1_result == TestResult::Pass { Some(kotlin_result.clone()) } else { None },
        });
        
        // Calculate test results
        let passed = test_details.iter().filter(|d| d.result == TestResult::Pass).count();
        let failed = test_details.iter().filter(|d| d.result == TestResult::Fail).count();
        let warnings = test_details.iter().filter(|d| d.result == TestResult::Warning).count();
        let total = test_details.len();
        
        println!("Complex Scenario Tests: {} passed, {} failed, {} warnings", passed, failed, warnings);
        
        Ok(Phase1IntegrationTestResult {
            test_category: TestCategory::ComplexScenarios,
            passed,
            failed,
            warnings,
            total,
            execution_time: start_time.elapsed(),
            details: test_details,
        })
    }
    
    /// Generate integration report
    fn generate_integration_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n=== Phase 1 Integration Test Report ===");
        
        for result in &self.results {
            println!("\n{} Tests:", match result.test_category {
                TestCategory::EnhancedInputPreview => "Enhanced Input Preview",
                TestCategory::EnhancedScrollablePreview => "Enhanced Scrollable Preview",
                TestCategory::EnhancedD1Runtime => "Enhanced D1 Runtime",
                TestCategory::BackwardCompatibility => "Backward Compatibility",
                TestCategory::ComplexScenarios => "Complex Scenarios",
            });
            
            let total_tests = result.total;
            let pass_rate = if total_tests > 0 {
                (result.passed as f64 / total_tests as f64 * 100.0)
            } else {
                0.0
            };
            
            println!("  Total Tests: {}", total_tests);
            println!("  Passed: {}", result.passed);
            println!("  Failed: {}", result.failed);
            println!("  Warnings: {}", result.warnings);
            println!("  Execution Time: {:.2}s", result.execution_time.as_secs_f64());
            println!("  Pass Rate: {:.1}%", pass_rate);
            
            // Print detailed test results
            for detail in &result.details {
                let status = match detail.result {
                    TestResult::Pass => "✅",
                    TestResult::Fail => "❌",
                    TestResult::Warning => "⚠️",
                };
                println!("    {}: {}{}", detail.test_name, status, if detail.result == TestResult::Fail { format!" (Error: {:?})", detail.error_message) } else { "".to_string() });
            }
        }
        
        // Overall summary
        let total_tests: usize = self.results.iter().map(|r| r.total).sum();
        let total_passed: usize = self.results.iter().map(|r| r.passed).sum();
        let total_failed: usize = self.results.iter().map(|r| r.failed).sum();
        let total_warnings: usize = self.results.iter().map(|r| r.warnings).sum();
        let overall_pass_rate = if total_tests > 0 {
            (total_passed as f64 / total_tests as f64 * 100.0)
        } else {
            0.0
        };
        
        println!("\n=== OVERALL INTEGRATION TEST SUMMARY ===");
        println!("Total Test Categories: {}", self.results.len());
        println!("Overall Total Tests: {}", total_tests);
        println!("Overall Passed: {}", total_passed);
        println!("Overall Failed: {}", total_failed);
        println!("Overall Warnings: {}", total_warnings);
        println!("Overall Pass Rate: {:.1}%", overall_pass_rate);
        
        if total_failed > 0 {
            println!("\n❌ INTEGRATION TESTS FAILED - Review output for details");
        } else {
            println!("\n✅ ALL INTEGRATION TESTS PASSED SUCCESSFULLY!");
        }
        
        Ok(())
    }
}

fn main() {
    // Create integration test configuration
    let config = Phase1IntegrationConfig {
        enable_input_preview_tests: true,
        enable_scrollable_preview_tests: true,
        enable_d1_runtime_tests: true,
        validate_backward_compatibility: true,
        test_complex_scenarios: true,
    };
    
    // Create integration test orchestrator
    let mut orchestrator = Phase1IntegrationTestOrchestrator::new(config);
    
    // Run integration tests
    if let Err(e) = orchestrator.run_all_tests() {
        eprintln!("Phase 1 Integration Tests failed: {}", e);
        std::process::exit(1);
    }
}
