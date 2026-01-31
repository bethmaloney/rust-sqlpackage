CREATE FUNCTION [dbo].[ScalarFunc7]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table8]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
