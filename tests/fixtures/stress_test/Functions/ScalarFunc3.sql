CREATE FUNCTION [dbo].[ScalarFunc3]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table4]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
