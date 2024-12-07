<?php
include 'db_connection.php'; // Include the database connection file

// Check if filename is provided
if (isset($_GET['name'])) {
    $name = $_GET['name'];

    // Prepare and execute the select query
    $sql = "SELECT downloaded FROM download WHERE name = ?";
    $stmt = $conn->prepare($sql);
    $stmt->bind_param("s", $name);
    $stmt->execute();
    $result = $stmt->get_result();

    // Check if record exists
    if ($result->num_rows > 0) {
        // Fetch the result
        $row = $result->fetch_assoc();
        $downloadCount = $row['downloaded'];
        echo "Download count for $name: $downloadCount";
    } else {
        // Record not found
        echo "No download count found for $name";
    }

    // Close statement
    $stmt->close();
} else {
    // name not provided
    echo "name not provided";
}

// Close connection
$conn->close();
?>