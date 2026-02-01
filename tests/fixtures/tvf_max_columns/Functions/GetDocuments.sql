-- Multi-statement table-valued function with MAX type columns
-- Tests that NVARCHAR(MAX), VARCHAR(MAX), and VARBINARY(MAX) columns
-- are correctly rendered with IsMax=True instead of Length=4294967295
CREATE FUNCTION [dbo].[GetDocuments](@CategoryId INT)
RETURNS @Results TABLE (
    Id INT,
    Title NVARCHAR(200),
    Content NVARCHAR(MAX),
    Description VARCHAR(MAX),
    BinaryData VARBINARY(MAX),
    ShortCode VARCHAR(10)
)
AS
BEGIN
    INSERT INTO @Results
    SELECT 1, N'Test', N'Long content', 'Long description', 0x00, 'ABC';
    RETURN;
END
GO
