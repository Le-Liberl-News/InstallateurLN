<?php
include 'db_connection.php'; // Include the database connection file

// Check if filename is provided
if (isset($_POST['filename'])) {
    $filename = $_POST['filename'];

    // Prepare and execute the update query
    $sql = "UPDATE download SET downloaded = downloaded + 1 WHERE filename = ?";
    $stmt = $conn->prepare($sql);
    $stmt->bind_param("s", $filename);

    if ($stmt->execute()) {
        // Update successful
        echo "Download count updated successfully for $filename";
    } else {
        // Update failed
        echo "Error updating download count: " . $conn->error;
    }

    // Close statement
    $stmt->close();
} else {
    // Filename not provided
    echo "Filename not provided";
}

// Close connection
$conn->close();
?>