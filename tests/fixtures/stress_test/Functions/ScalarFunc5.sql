CREATE FUNCTION [dbo].[ScalarFunc5]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table6]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
