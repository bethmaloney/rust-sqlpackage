CREATE FUNCTION [dbo].[ScalarFunc2]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table3]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
