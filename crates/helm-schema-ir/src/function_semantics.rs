pub(crate) fn is_files_get(function: &str) -> bool {
    function == "Files.Get" || function.ends_with(".Files.Get")
}
