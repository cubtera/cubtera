// TODO: Implement comprehensive file system tests
// - copy_folder tests with temp directories
// - copy_all_files_in_folder tests
// - check_path tests
// - create_dir_all tests
// - remove_file/remove_dir_all tests
// - read_to_string/write_string tests
// - get_metadata tests
// - error scenarios tests
// - permission tests
// - large file tests
// - concurrent access tests 

#[cfg(test)]
mod tests {
    use crate::tools::fs::*;
    use crate::tools::error::ToolsError;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_copy_all_files_in_folder_success() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create test files
        fs::write(src_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(src_dir.path().join("file2.txt"), "content2").unwrap();
        fs::create_dir(src_dir.path().join("subdir")).unwrap(); // Should be ignored

        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_ok());
        assert!(dst_dir.path().join("file1.txt").exists());
        assert!(dst_dir.path().join("file2.txt").exists());
        assert!(!dst_dir.path().join("subdir").exists()); // Directories not copied

        let content1 = fs::read_to_string(dst_dir.path().join("file1.txt")).unwrap();
        assert_eq!(content1, "content1");
    }

    #[test]
    fn test_copy_all_files_in_folder_overwrite_false() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create source file
        fs::write(src_dir.path().join("file.txt"), "new_content").unwrap();
        
        // Create existing destination file
        fs::write(dst_dir.path().join("file.txt"), "old_content").unwrap();

        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            false, // Don't overwrite
        );

        assert!(result.is_ok());
        
        // File should not be overwritten
        let content = fs::read_to_string(dst_dir.path().join("file.txt")).unwrap();
        assert_eq!(content, "old_content");
    }

    #[test]
    fn test_copy_all_files_in_folder_overwrite_true() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create source file
        fs::write(src_dir.path().join("file.txt"), "new_content").unwrap();
        
        // Create existing destination file
        fs::write(dst_dir.path().join("file.txt"), "old_content").unwrap();

        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true, // Overwrite
        );

        assert!(result.is_ok());
        
        // File should be overwritten
        let content = fs::read_to_string(dst_dir.path().join("file.txt")).unwrap();
        assert_eq!(content, "new_content");
    }

    #[test]
    fn test_copy_all_files_in_folder_source_not_exists() {
        let dst_dir = TempDir::new().unwrap();
        let non_existent = PathBuf::from("/non/existent/path");

        let result = copy_all_files_in_folder(
            non_existent,
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_copy_all_files_in_folder_source_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let dst_dir = TempDir::new().unwrap();

        let result = copy_all_files_in_folder(
            file_path,
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::InvalidInput { .. } => (),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_copy_folder_recursive() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create nested structure
        fs::write(src_dir.path().join("root_file.txt"), "root").unwrap();
        fs::create_dir(src_dir.path().join("subdir")).unwrap();
        fs::write(src_dir.path().join("subdir/sub_file.txt"), "sub").unwrap();
        fs::create_dir(src_dir.path().join("subdir/nested")).unwrap();
        fs::write(src_dir.path().join("subdir/nested/deep_file.txt"), "deep").unwrap();

        let result = copy_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_ok());
        
        // Check all files were copied
        assert!(dst_dir.path().join("root_file.txt").exists());
        assert!(dst_dir.path().join("subdir/sub_file.txt").exists());
        assert!(dst_dir.path().join("subdir/nested/deep_file.txt").exists());

        // Check content
        let content = fs::read_to_string(dst_dir.path().join("subdir/nested/deep_file.txt")).unwrap();
        assert_eq!(content, "deep");
    }

    #[test]
    fn test_check_path_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let result = check_path(file_path.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file_path);
    }

    #[test]
    fn test_check_path_not_exists() {
        let non_existent = PathBuf::from("/non/existent/path");
        let result = check_path(non_existent);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_create_dir_all() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a/b/c/d");

        let result = create_dir_all(&nested_path);
        assert!(result.is_ok());
        assert!(nested_path.exists());
        assert!(nested_path.is_dir());
    }

    #[test]
    fn test_remove_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");
        fs::write(&file_path, "content").unwrap();

        assert!(file_path.exists());

        let result = remove_file(&file_path);
        assert!(result.is_ok());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_file_not_exists() {
        let non_existent = PathBuf::from("/non/existent/file.txt");
        let result = remove_file(&non_existent);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_remove_file_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("directory");
        fs::create_dir(&dir_path).unwrap();

        let result = remove_file(&dir_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::InvalidInput { .. } => (),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_remove_dir_all() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("to_delete");
        fs::create_dir_all(&dir_path.join("nested")).unwrap();
        fs::write(dir_path.join("file.txt"), "content").unwrap();
        fs::write(dir_path.join("nested/file.txt"), "nested").unwrap();

        assert!(dir_path.exists());

        let result = remove_dir_all(&dir_path);
        assert!(result.is_ok());
        assert!(!dir_path.exists());
    }

    #[test]
    fn test_read_to_string() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello, world!\nMultiline content\nWith unicode: 🚀";
        fs::write(&file_path, content).unwrap();

        let result = read_to_string(&file_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_read_to_string_not_exists() {
        let non_existent = PathBuf::from("/non/existent/file.txt");
        let result = read_to_string(&non_existent);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_read_to_string_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = read_to_string(&temp_dir.path().to_path_buf());
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::InvalidInput { .. } => (),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_write_string() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let content = "Test content\nWith newlines\nAnd unicode: 🎉";

        let result = write_string(&file_path, content);
        assert!(result.is_ok());
        
        let read_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_write_string_create_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/deep/file.txt");
        let content = "Content in nested directory";

        let result = write_string(&file_path, content);
        assert!(result.is_ok());
        
        assert!(file_path.exists());
        let read_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_get_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Test content for metadata";
        fs::write(&file_path, content).unwrap();

        let result = get_metadata(&file_path);
        assert!(result.is_ok());
        
        let metadata = result.unwrap();
        assert!(metadata.is_file());
        assert_eq!(metadata.len(), content.len() as u64);
    }

    #[test]
    fn test_get_metadata_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = get_metadata(&temp_dir.path().to_path_buf());
        
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.is_dir());
    }

    #[test]
    fn test_get_metadata_not_exists() {
        let non_existent = PathBuf::from("/non/existent/file.txt");
        let result = get_metadata(&non_existent);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    // Performance tests
    #[test]
    fn test_performance_copy_many_files() {
        use std::time::Instant;
        
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create 100 small files
        for i in 0..100 {
            fs::write(src_dir.path().join(format!("file_{}.txt", i)), format!("content {}", i)).unwrap();
        }

        let start = Instant::now();
        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true,
        );
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert!(duration.as_millis() < 1000, "Copying 100 files took too long: {:?}", duration);
        
        // Verify all files were copied
        for i in 0..100 {
            assert!(dst_dir.path().join(format!("file_{}.txt", i)).exists());
        }
    }

    // Edge cases
    #[test]
    fn test_empty_directory_copy() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_ok());
        // Should succeed even with empty directory
    }

    #[test]
    fn test_unicode_filenames() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create files with unicode names
        fs::write(src_dir.path().join("файл.txt"), "русский текст").unwrap();
        fs::write(src_dir.path().join("文件.txt"), "中文内容").unwrap();
        fs::write(src_dir.path().join("🚀.txt"), "emoji filename").unwrap();

        let result = copy_all_files_in_folder(
            src_dir.path().to_path_buf(),
            &dst_dir.path().to_path_buf(),
            true,
        );

        assert!(result.is_ok());
        assert!(dst_dir.path().join("файл.txt").exists());
        assert!(dst_dir.path().join("文件.txt").exists());
        assert!(dst_dir.path().join("🚀.txt").exists());

        let content = fs::read_to_string(dst_dir.path().join("файл.txt")).unwrap();
        assert_eq!(content, "русский текст");
    }
} 