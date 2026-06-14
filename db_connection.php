<?php
// Database configuration
$servername = "localhost"; // Change this to your database server name
$username = "twnkek290001"; // Change this to your database username
$password = "Rh5mkKAllN"; // Change this to your database password
$database = "capelDB"; // Change this to your database name

// Create connection
$conn = new mysqli($servername, $username, $password, $database);

// Check connection
if ($conn->connect_error) {
    die("Connection failed: " . $conn->connect_error);
}
?>
